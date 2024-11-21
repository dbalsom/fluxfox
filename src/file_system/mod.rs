/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024 Daniel Balsom

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
use thiserror::Error;

#[cfg(feature = "fat")]
pub mod fat;

#[derive(Clone, Debug, Error)]
pub enum FileSystemError {
    #[error("An IO error occurred reading or writing the disk image: {0}")]
    IoError(String),
    #[error("The filesystem is not mounted")]
    NotMountedError,
    #[error("An error occurred mounting the file system: {0}")]
    MountError(String),
    #[error("An error occurred reading a file: {0}")]
    ReadError(String),
}

#[derive(Clone, Debug)]
pub enum FileEntryType {
    File,
    Directory,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    e_type:   FileEntryType,
    pub name: String,
    pub path: String,
    pub size: u64,
}

impl FileEntry {
    pub fn is_file(&self) -> bool {
        matches!(self.e_type, FileEntryType::File)
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.e_type, FileEntryType::Directory)
    }

    pub fn ext(&self) -> Option<&str> {
        let parts: Vec<&str> = self.name.split('.').collect();
        if parts.len() > 1 {
            let ext = parts[parts.len() - 1];
            //log::debug!("ext: {}", ext);
            Some(ext)
        }
        else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub enum FileTreeNode {
    File(FileEntry),
    Directory { fs: FileEntry, children: Vec<FileTreeNode> },
}

impl Default for FileTreeNode {
    fn default() -> Self {
        FileTreeNode::Directory {
            fs: FileEntry {
                e_type: FileEntryType::Directory,
                name:   "/".to_string(),
                path:   "/".to_string(),
                size:   0,
            },
            children: Vec::new(),
        }
    }
}

impl FileTreeNode {
    pub fn sub_dir_ct(&self) -> usize {
        if let FileTreeNode::Directory { children, .. } = self {
            children
                .iter()
                .filter(|c| matches!(c, FileTreeNode::Directory { fs: _, children: _ }))
                .count()
        }
        else {
            0
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, FileTreeNode::File(_))
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, FileTreeNode::Directory { fs: _, children: _ })
    }

    /// Returns a vector of `FileEntry` for the given path if the path exists.
    pub fn dir(&self, path: &str) -> Option<Vec<FileEntry>> {
        // Split the path into components for navigation
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        self.resolve_dir(&components)
    }

    /// Helper function to resolve a directory from a list of components
    fn resolve_dir(&self, components: &[&str]) -> Option<Vec<FileEntry>> {
        match self {
            FileTreeNode::File(_) => None, // A file cannot have children
            FileTreeNode::Directory { fs: _, children } => {
                if components.is_empty() {
                    // If no more components, return the current directory's children as FileEntry
                    Some(
                        children
                            .iter()
                            .map(|child| match child {
                                FileTreeNode::File(file) => file.clone(),
                                FileTreeNode::Directory { fs, .. } => fs.clone(),
                            })
                            .collect(),
                    )
                }
                else {
                    // Otherwise, find the next component in the children and continue resolving
                    let next_component = components[0];
                    let remaining_components = &components[1..];
                    for child in children {
                        if let FileTreeNode::Directory { fs, .. } = child {
                            if fs.name == next_component {
                                return child.resolve_dir(remaining_components);
                            }
                        }
                    }
                    None // No matching directory found
                }
            }
        }
    }
}
