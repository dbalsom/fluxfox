/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------
*/

//! Build a filesystem tree from a ZIP archive without extracting it to disk.

use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{Cursor, Read, Seek},
    path::{Component, Path},
};

use crate::file_system::{
    file_tree::{prioritize_boot_system_files, FileEntryType},
    is_boot_sector_filename,
    FileEntry,
    FileEntryAttributes,
    FileSystemError,
    FileTreeNode,
    FsDateTime,
};

const MAX_FILE_SIZE: u64 = 100_000_000;

#[derive(Default)]
struct ArchiveDirectory {
    modified: Option<FsDateTime>,
    children: BTreeMap<String, ArchiveNode>,
}

struct ArchiveFile {
    archive_index: usize,
    size: u64,
    modified: Option<FsDateTime>,
}

enum ArchiveNode {
    File(ArchiveFile),
    Directory(ArchiveDirectory),
}

/// A ZIP archive paired with the image-relative file tree represented by its entries.
pub(crate) struct ZipFileSource<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
    indices: HashMap<String, usize>,
    tree:    FileTreeNode,
}

impl ZipFileSource<File> {
    pub(crate) fn open(path: impl AsRef<Path>, recursive: bool) -> Result<Self, FileSystemError> {
        let file = File::open(path)?;
        Self::from_reader(file, recursive)
    }
}

impl ZipFileSource<Cursor<Vec<u8>>> {
    pub(crate) fn from_bytes(bytes: Vec<u8>, recursive: bool) -> Result<Self, FileSystemError> {
        Self::from_reader(Cursor::new(bytes), recursive)
    }
}

impl<R: Read + Seek> ZipFileSource<R> {
    fn from_reader(reader: R, recursive: bool) -> Result<Self, FileSystemError> {
        let mut archive = zip::ZipArchive::new(reader)?;
        let mut root = ArchiveDirectory::default();

        for archive_index in 0..archive.len() {
            let entry = archive.by_index(archive_index)?;
            let entry_name = entry.name().to_string();
            let enclosed_path = entry
                .enclosed_name()
                .ok_or_else(|| FileSystemError::UnsupportedFileObject(entry_name.clone()))?;
            let components = path_components(&enclosed_path)?;

            if components.is_empty() {
                continue;
            }
            if !recursive && components.len() != 1 {
                continue;
            }

            // A root-level boot-sector binary configures the image and is not copied into FAT.
            if components.len() == 1 && entry.is_file() && is_boot_sector_filename(&components[0]) {
                continue;
            }

            let modified = entry.last_modified().map(zip_datetime);
            if entry.is_symlink() {
                return Err(FileSystemError::UnsupportedFileObject(entry_name));
            }
            if entry.is_dir() {
                if recursive {
                    insert_directory(&mut root, &components, modified, &entry_name)?;
                }
            }
            else if entry.is_file() {
                insert_file(
                    &mut root,
                    &components,
                    ArchiveFile {
                        archive_index,
                        size: entry.size(),
                        modified,
                    },
                    &entry_name,
                )?;
            }
            else {
                return Err(FileSystemError::UnsupportedFileObject(entry_name));
            }
        }

        let mut indices = HashMap::new();
        let mut tree = directory_to_file_tree(root, "/", "", &mut indices);
        prioritize_boot_system_files(&mut tree)?;
        Ok(Self { archive, indices, tree })
    }

    pub(crate) fn tree(&self) -> &FileTreeNode {
        &self.tree
    }

    pub(crate) fn read_file(&mut self, entry: &FileEntry) -> Result<Vec<u8>, FileSystemError> {
        let index = self
            .indices
            .get(entry.path())
            .copied()
            .ok_or_else(|| FileSystemError::PathNotFound(entry.path().to_string()))?;
        let mut archive_file = self.archive.by_index(index)?;

        // Keep malformed or hostile archives from allocating an unreasonable amount of memory.
        if archive_file.size() > MAX_FILE_SIZE {
            return Err(FileSystemError::ArchiveError(format!(
                "archive member is too large: {}",
                entry.path()
            )));
        }

        let mut data = Vec::with_capacity(archive_file.size() as usize);
        archive_file
            .read_to_end(&mut data)
            .map_err(|error| FileSystemError::ArchiveError(error.to_string()))?;
        Ok(data)
    }
}

fn path_components(path: &Path) -> Result<Vec<String>, FileSystemError> {
    path.components()
        .filter_map(|component| match component {
            Component::CurDir => None,
            Component::Normal(name) => Some(
                name.to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| FileSystemError::UnsupportedFileObject(path.display().to_string())),
            ),
            _ => Some(Err(FileSystemError::UnsupportedFileObject(path.display().to_string()))),
        })
        .collect()
}

fn zip_datetime(datetime: zip::DateTime) -> FsDateTime {
    FsDateTime {
        year: datetime.year(),
        month: datetime.month(),
        day: datetime.day(),
        hour: datetime.hour(),
        minute: datetime.minute(),
        second: datetime.second(),
        millisecond: 0,
    }
}

fn insert_directory(
    directory: &mut ArchiveDirectory,
    components: &[String],
    modified: Option<FsDateTime>,
    archive_name: &str,
) -> Result<(), FileSystemError> {
    let Some((name, remaining)) = components.split_first()
    else {
        return Ok(());
    };

    let node = directory
        .children
        .entry(name.clone())
        .or_insert_with(|| ArchiveNode::Directory(ArchiveDirectory::default()));
    let ArchiveNode::Directory(child) = node
    else {
        return Err(FileSystemError::ArchiveError(format!(
            "archive entry conflicts with a file: {archive_name}"
        )));
    };

    if remaining.is_empty() {
        child.modified = modified;
        Ok(())
    }
    else {
        insert_directory(child, remaining, modified, archive_name)
    }
}

fn insert_file(
    directory: &mut ArchiveDirectory,
    components: &[String],
    file: ArchiveFile,
    archive_name: &str,
) -> Result<(), FileSystemError> {
    let (name, parents) = components
        .split_last()
        .expect("archive paths are checked for empty components");
    let mut destination = directory;

    for parent in parents {
        let node = destination
            .children
            .entry(parent.clone())
            .or_insert_with(|| ArchiveNode::Directory(ArchiveDirectory::default()));
        let ArchiveNode::Directory(child) = node
        else {
            return Err(FileSystemError::ArchiveError(format!(
                "archive entry has a file as a parent: {archive_name}"
            )));
        };
        destination = child;
    }

    if destination
        .children
        .insert(name.clone(), ArchiveNode::File(file))
        .is_some()
    {
        return Err(FileSystemError::ArchiveError(format!(
            "archive contains duplicate or conflicting entry: {archive_name}"
        )));
    }
    Ok(())
}

fn directory_to_file_tree(
    directory: ArchiveDirectory,
    name: &str,
    parent_path: &str,
    indices: &mut HashMap<String, usize>,
) -> FileTreeNode {
    let path = if parent_path.is_empty() {
        if name == "/" {
            "/".to_string()
        }
        else {
            name.to_string()
        }
    }
    else {
        format!("{parent_path}/{name}")
    };
    let mut children = Vec::with_capacity(directory.children.len());

    for (child_name, child) in directory.children {
        let child_path = if path == "/" {
            child_name.clone()
        }
        else {
            format!("{path}/{child_name}")
        };
        match child {
            ArchiveNode::File(file) => {
                indices.insert(child_path.clone(), file.archive_index);
                children.push(FileTreeNode::File(FileEntry {
                    e_type: FileEntryType::File,
                    short_name: child_name.clone(),
                    long_name: Some(child_name),
                    path: child_path,
                    size: file.size,
                    created: None,
                    modified: file.modified,
                    attributes: FileEntryAttributes::default(),
                }));
            }
            ArchiveNode::Directory(child_directory) => {
                children.push(directory_to_file_tree(
                    child_directory,
                    &child_name,
                    if path == "/" { "" } else { &path },
                    indices,
                ));
            }
        }
    }

    FileTreeNode::Directory {
        dfe: FileEntry {
            e_type: FileEntryType::Directory,
            short_name: name.to_string(),
            long_name: Some(name.to_string()),
            path,
            size: 0,
            created: None,
            modified: directory.modified,
            attributes: FileEntryAttributes::default(),
        },
        children,
    }
}
