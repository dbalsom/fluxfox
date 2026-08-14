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

//! Module for native filesystem operations.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    file_system::{
        file_tree::{prioritize_boot_system_files, FileEntryType},
        is_boot_sector_filename,
        FileEntry,
        FileEntryAttributes,
        FileSystemError,
        FileTreeNode,
        FsDateTime,
    },
    FoxHashSet,
};

pub fn list_files_relative(path: impl AsRef<Path>, recursive: bool) -> Result<Vec<String>, FileSystemError> {
    let path = PathBuf::from(path.as_ref());
    let files = list_files(&path, recursive)?;
    let base_path = path.to_string_lossy().to_string();
    Ok(files
        .iter()
        .map(|f| f.trim_start_matches(&base_path).to_string())
        .collect())
}

pub fn list_files(path: impl AsRef<Path>, recursive: bool) -> Result<Vec<String>, FileSystemError> {
    let path = PathBuf::from(path.as_ref());
    let dir = fs::read_dir(&path)?;
    let mut files = Vec::new();
    let mut visited_dirs = FoxHashSet::new();

    //let base_path = path.to_string_lossy().to_string();

    if recursive {
        list_files_recursive(dir, &mut files, &mut visited_dirs)?;
    }
    else {
        for entry_res in dir {
            let entry = entry_res?;
            let file_type = entry.file_type()?;
            if file_type.is_file() {
                files.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }

    Ok(files)
}

pub fn list_files_recursive(
    dir: fs::ReadDir,
    files: &mut Vec<String>,
    visited_dirs: &mut FoxHashSet<PathBuf>,
) -> Result<(), FileSystemError> {
    for entry_res in dir {
        // If we can’t read an entry, fail immediately
        let entry = entry_res?;

        // If we can’t get filetype, fail immediately
        let file_type = entry.file_type()?;

        let path = entry.path();
        let name_str = entry.file_name().to_string_lossy().to_string();

        if file_type.is_dir() {
            // Skip "." and ".." to avoid going in circles
            if name_str == "." || name_str == ".." {
                continue;
            }

            // Attempt to canonicalize to detect symlinks
            let real_path = path.canonicalize()?;

            // If we’ve seen this directory before, we have a cycle
            if visited_dirs.contains(&real_path) {
                return Err(FileSystemError::CycleError);
            }
            visited_dirs.insert(real_path);

            // Descend into the subdirectory
            let sub_dir = fs::read_dir(&path)?;
            list_files_recursive(sub_dir, files, visited_dirs)?;
        }
        else if file_type.is_file() {
            let file_path_string = path.to_string_lossy().to_string();
            log::trace!("Adding file: {}", file_path_string);
            files.push(file_path_string);
        }
        else {
            // Not a file or directory?
            return Err(FileSystemError::UnsupportedFileObject(
                path.to_string_lossy().to_string(),
            ));
        }
    }
    Ok(())
}

/// Build a recursive, image-relative tree from a native directory.
pub fn build_file_tree(path: impl AsRef<Path>) -> Result<FileTreeNode, FileSystemError> {
    build_file_tree_with_options(path, true)
}

/// Build an image-relative tree from a native directory.
///
/// Directory entries are sorted before traversal so consumers can reproduce the same insertion
/// order. Symlinks and other non-file objects are rejected instead of being followed implicitly.
pub fn build_file_tree_with_options(path: impl AsRef<Path>, recursive: bool) -> Result<FileTreeNode, FileSystemError> {
    let root = path.as_ref();
    if !root.is_dir() {
        return Err(FileSystemError::PathNotFound(root.display().to_string()));
    }

    let canonical_root = root.canonicalize()?;
    let mut visited_dirs = FoxHashSet::new();
    visited_dirs.insert(canonical_root);

    let mut tree = build_file_tree_recursive(root, None, &mut Vec::new(), recursive, &mut visited_dirs)?;
    prioritize_boot_system_files(&mut tree)?;
    Ok(tree)
}

fn build_file_tree_recursive(
    dir_path: &Path,
    dir_entry: Option<&fs::DirEntry>,
    path_stack: &mut Vec<String>,
    recursive: bool,
    visited_dirs: &mut FoxHashSet<PathBuf>,
) -> Result<FileTreeNode, FileSystemError> {
    let dir_name = match dir_entry {
        Some(entry) => Some(
            entry
                .file_name()
                .into_string()
                .map_err(|name| FileSystemError::UnsupportedFileObject(name.to_string_lossy().into_owned()))?,
        ),
        None => None,
    };

    if let Some(name) = &dir_name {
        path_stack.push(name.clone());
    }

    let mut entries = fs::read_dir(dir_path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut children = Vec::new();
    for entry in entries {
        let entry_path = entry.path();
        let entry_name = entry
            .file_name()
            .into_string()
            .map_err(|name| FileSystemError::UnsupportedFileObject(name.to_string_lossy().into_owned()))?;
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            return Err(FileSystemError::UnsupportedFileObject(entry_path.display().to_string()));
        }

        let full_path = if path_stack.is_empty() {
            entry_name.clone()
        }
        else {
            format!("{}/{}", path_stack.join("/"), entry_name)
        };

        if file_type.is_dir() {
            if !recursive {
                continue;
            }

            let canonical_path = entry_path.canonicalize()?;
            if !visited_dirs.insert(canonical_path) {
                return Err(FileSystemError::CycleError);
            }

            log::debug!("Descending into dir: {}", full_path);
            let child = build_file_tree_recursive(&entry_path, Some(&entry), path_stack, true, visited_dirs)?;
            children.push(child);
        }
        else if file_type.is_file() {
            // A root-level boot-sector binary configures the image and is not copied into FAT.
            if path_stack.is_empty() && is_boot_sector_filename(&entry_name) {
                continue;
            }
            let metadata = entry.metadata()?;
            children.push(FileTreeNode::File(FileEntry {
                e_type: FileEntryType::File,
                short_name: entry_name.clone(),
                long_name: Some(entry_name),
                size: metadata.len(),
                path: full_path,
                created: metadata.created().ok().and_then(|time| FsDateTime::try_from(time).ok()),
                modified: metadata
                    .modified()
                    .ok()
                    .and_then(|time| FsDateTime::try_from(time).ok()),
                attributes: FileEntryAttributes::default(),
            }));
        }
        else {
            return Err(FileSystemError::UnsupportedFileObject(entry_path.display().to_string()));
        }
    }

    let node_path = if path_stack.is_empty() {
        "/".to_string()
    }
    else {
        path_stack.join("/")
    };
    let metadata = dir_entry.and_then(|entry| entry.metadata().ok());
    let node = FileTreeNode::Directory {
        dfe: FileEntry {
            e_type: FileEntryType::Directory,
            short_name: dir_name.clone().unwrap_or_else(|| "/".to_string()),
            long_name: Some(dir_name.unwrap_or_else(|| "/".to_string())),
            path: node_path,
            size: 0,
            created: metadata
                .as_ref()
                .and_then(|metadata| metadata.created().ok())
                .and_then(|time| FsDateTime::try_from(time).ok()),
            modified: metadata
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| FsDateTime::try_from(time).ok()),
            attributes: FileEntryAttributes::default(),
        },
        children,
    };

    if dir_entry.is_some() {
        path_stack.pop();
    }

    Ok(node)
}
