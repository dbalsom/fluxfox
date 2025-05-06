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
    file_system::{file_tree::FileEntryType, FileEntry, FileSystemError, FileTreeNode, FsDateTime},
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

pub fn build_file_tree(path: impl AsRef<Path>) -> Result<FileTreeNode, FileSystemError> {
    let path = PathBuf::from(path.as_ref());
    let root_dir = fs::read_dir(&path)?;
    let mut path_stack = Vec::new();
    build_file_tree_recursive(None, root_dir, &mut path_stack)
}

pub fn build_file_tree_recursive(
    dir_entry: Option<&fs::DirEntry>,
    dir: fs::ReadDir,
    path_stack: &mut Vec<String>,
) -> Result<FileTreeNode, FileSystemError> {
    let mut children = Vec::new();

    if let Some(dir_entry) = dir_entry {
        path_stack.push(dir_entry.file_name().to_string_lossy().to_string());
    }

    for entry in dir {
        match entry {
            Ok(entry) => {
                let entry_name = entry.file_name().to_string_lossy().to_string();
                let entry_size = entry.metadata().map(|m| m.len()).unwrap_or(0);

                let full_path = if path_stack.is_empty() {
                    entry_name.clone()
                }
                else {
                    format!("{}/{}", path_stack.join("/"), entry_name)
                };

                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        // Ignore the current and parent directory entries to avoid infinite recursion
                        if entry_name == "." || entry_name == ".." {
                            continue;
                        }
                        log::debug!("Descending into dir: {}", full_path);
                        let sub_dir = fs::read_dir(entry.path())?;
                        let new_node = build_file_tree_recursive(Some(&entry), sub_dir, path_stack)?;
                        log::debug!("Adding child directory: {}", new_node.path());
                        children.push(new_node);
                    }
                    else if file_type.is_file() {
                        // log::debug!(
                        //     "adding file: {} modified date: {}",
                        //     entry_name,
                        //     FsDateTime::from(entry.modified())
                        // );
                        children.push(FileTreeNode::File(FileEntry {
                            e_type: FileEntryType::File,
                            short_name: entry_name,
                            long_name: Some(entry.file_name().to_string_lossy().to_string()),
                            size: entry_size,
                            path: full_path,
                            created: None, // Created date was not implemented by DOS. Added in NT + later
                            modified: entry
                                .metadata()
                                .ok()
                                .and_then(|md| md.modified().ok())
                                .and_then(|st| FsDateTime::try_from(st).ok()),
                        }));
                    }
                }
            }
            Err(e) => {
                log::error!("error reading directory entry: {}", e);
                continue;
            }
        }
    }

    let node = FileTreeNode::Directory {
        dfe: FileEntry {
            e_type: FileEntryType::Directory,
            short_name: dir_entry
                .map(|e| e.file_name().to_string_lossy().to_string())
                .unwrap_or_default(),
            long_name: dir_entry
                .map(|e| Some(e.file_name().to_string_lossy().to_string()))
                .unwrap_or_else(|| None),
            path: if path_stack.len() < 1 {
                "/".to_string()
            }
            else {
                path_stack.join("/")
            },
            size: 0, // Directory size can be calculated if needed
            created: None,
            modified: dir_entry
                .and_then(|e| e.metadata().ok())
                .and_then(|md| md.modified().ok())
                .and_then(|st| FsDateTime::try_from(st).ok()),
        },
        children,
    };

    // Pop the current directory name from the path stack
    path_stack.pop();

    Ok(node)
}
