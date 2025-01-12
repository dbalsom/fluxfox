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
use crate::file_system::date_time::FsDateTime;
use std::fmt::{Display, Formatter, Result};

#[derive(Copy, Clone, Debug)]
pub enum FileEntryType {
    File,
    Directory,
}

#[derive(Copy, Clone, Default, Debug)]
pub enum FileNameType {
    #[default]
    Short,
    Long,
}

#[derive(Clone)]
pub struct FileEntry {
    pub(crate) e_type: FileEntryType,
    pub(crate) short_name: String,
    pub(crate) long_name: Option<String>,
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) created: Option<FsDateTime>,
    pub(crate) modified: Option<FsDateTime>,
}

impl Display for FileEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "{} {:>10} {}",
            self.modified.as_ref().unwrap_or(&FsDateTime::default()),
            self.size,
            self.short_name
        )
    }
}

impl FileEntry {
    /// Returns the name of the file, of the requested `FileNameType`.
    /// # Arguments
    /// * `name_type` - The type of name to return.
    /// # Returns
    /// * `Some(String)` - The name of the file, if the specified filename type exists.
    /// * `None` - If the specified filename type does not exist.
    pub fn name(&self, name_type: FileNameType) -> Option<&str> {
        match name_type {
            FileNameType::Short => Some(&self.short_name),
            FileNameType::Long => self.long_name.as_deref(),
        }
    }

    /// Returns the short name of the file.
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// Returns the `FileEntryType` of the file entry.
    pub fn entry_type(&self) -> FileEntryType {
        self.e_type
    }

    /// Returns the full short path of the file.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the size of the file as u64 in bytes, or 0 if the entry is a directory.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns `true` if the entry is a file.
    /// # Returns
    /// * `true` - If the entry is a file.
    /// * `false` - If the entry is a directory.
    pub fn is_file(&self) -> bool {
        matches!(self.e_type, FileEntryType::File)
    }

    /// Returns `true` if the entry is a directory.
    /// # Returns
    /// * `true` - If the entry is a directory.
    /// * `false` - If the entry is a file.
    pub fn is_dir(&self) -> bool {
        matches!(self.e_type, FileEntryType::Directory)
    }

    /// Return the extension of the file, if it exists.
    /// # Returns
    /// * `Some(&str)` - The extension of the file.
    /// * `None` - If the file does not have an extension.
    pub fn ext(&self) -> Option<&str> {
        let parts: Vec<&str> = self.short_name.split('.').collect();
        if parts.len() > 1 {
            let ext = parts[parts.len() - 1];
            //log::debug!("ext: {}", ext);
            Some(ext)
        }
        else {
            None
        }
    }

    pub fn modified(&self) -> Option<&FsDateTime> {
        self.modified.as_ref()
    }

    pub fn created(&self) -> Option<&FsDateTime> {
        self.created.as_ref()
    }
}

#[derive(Clone)]
pub enum FileTreeNode {
    File(FileEntry),
    Directory { dfe: FileEntry, children: Vec<FileTreeNode> },
}

impl Default for FileTreeNode {
    fn default() -> Self {
        FileTreeNode::Directory {
            dfe: FileEntry {
                e_type: FileEntryType::Directory,
                short_name: "/".to_string(),
                long_name: Some("/".to_string()),
                path: "/".to_string(),
                size: 0,
                created: None,
                modified: None,
            },
            children: Vec::new(),
        }
    }
}

impl Display for FileTreeNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            FileTreeNode::File(entry) => write!(f, "{}", entry),
            FileTreeNode::Directory { dfe, .. } => {
                write!(f, "{}", dfe)
            }
        }
    }
}

impl FileTreeNode {
    pub fn sub_dir_ct(&self) -> usize {
        if let FileTreeNode::Directory { children, .. } = self {
            children
                .iter()
                .filter(|c| matches!(c, FileTreeNode::Directory { dfe: _, children: _ }))
                .count()
        }
        else {
            0
        }
    }

    /// Returns `true` if the current node represents a file.
    pub fn is_file(&self) -> bool {
        matches!(self, FileTreeNode::File(_))
    }

    /// Returns `true` if the current node represents a directory.
    pub fn is_dir(&self) -> bool {
        matches!(self, FileTreeNode::Directory { dfe: _, children: _ })
    }

    /// Returns a vector of [FileEntry] for the given path, if the path exists, or else `None`.
    pub fn dir(&self, path: &str) -> Option<Vec<FileEntry>> {
        // Split the path into components for navigation
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        self.resolve_dir(false, &components)
    }

    /// Walk the file tree from this node and call the provided function for each [FileEntry] found.
    pub fn for_each_file(&self, recursive: bool, f: &mut impl FnMut(&FileEntry)) {
        match self {
            FileTreeNode::File(file) => f(file),
            FileTreeNode::Directory { children, .. } if recursive => {
                for child in children {
                    child.for_each_file(true, f);
                }
            }
            _ => {}
        }
    }

    /// Returns a vector of file names recursively from the current node, of the specified [FileNameType].
    /// If [FileNameType::Long] is requested, but a long filename does not exist, the short filename
    /// will be returned instead.
    pub fn file_names(&self, recursive: bool, name_type: FileNameType) -> Vec<String> {
        let mut names = Vec::new();
        match name_type {
            FileNameType::Short => {
                self.for_each_file(recursive, &mut |file_entry| {
                    names.push(file_entry.short_name.clone());
                });
            }
            FileNameType::Long => {
                self.for_each_file(recursive, &mut |file_entry| {
                    if file_entry.long_name.is_none() {
                        names.push(file_entry.short_name.clone());
                    }
                    else if let Some(name) = file_entry.long_name.as_ref() {
                        names.push(name.clone());
                    }
                });
            }
        }
        names
    }

    /// Returns a vector of file paths for the given path, of the specified [FileNameType].
    /// If [FileNameType::Long] is requested, but a long filename does not exist, the short path
    /// will be returned instead.
    pub fn file_paths(&self, recursive: bool, name_type: FileNameType) -> Vec<String> {
        let mut names = Vec::new();
        match name_type {
            FileNameType::Short | FileNameType::Long => {
                self.for_each_file(recursive, &mut |file_entry| {
                    names.push(file_entry.path.clone());
                });
            }
        }
        names
    }

    pub fn node(&self, path: &str) -> Option<&FileTreeNode> {
        // Split the path into components for navigation
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        self.resolve_node(&components)
    }

    /// Helper function to resolve a FileTreeNode from a list of components
    fn resolve_node(&self, components: &[&str]) -> Option<&FileTreeNode> {
        match self {
            FileTreeNode::File(_) => None, // A file cannot have children
            FileTreeNode::Directory { dfe: _, children } => {
                if components.is_empty() {
                    // If no more components, we have resolved the path
                    Some(self)
                }
                else {
                    // Otherwise, find the next component in the children and continue resolving
                    let next_component = components[0];
                    let remaining_components = &components[1..];
                    for child in children {
                        if let FileTreeNode::Directory { dfe, .. } = child {
                            if dfe.short_name == next_component {
                                return child.resolve_node(remaining_components);
                            }
                        }
                    }
                    None // No matching directory found
                }
            }
        }
    }

    /// Helper function to resolve a directory from a list of components
    fn resolve_dir(&self, recursive: bool, components: &[&str]) -> Option<Vec<FileEntry>> {
        match self {
            FileTreeNode::File(_) => None, // A file cannot have children
            FileTreeNode::Directory { dfe: _, children } => {
                if components.is_empty() {
                    // If no more components, return the current directory's children as FileEntry
                    Some(
                        children
                            .iter()
                            .map(|child| match child {
                                FileTreeNode::File(file) => file.clone(),
                                FileTreeNode::Directory { dfe, .. } => dfe.clone(),
                            })
                            .collect(),
                    )
                }
                else if recursive {
                    // Otherwise, find the next component in the children and continue resolving
                    let next_component = components[0];
                    let remaining_components = &components[1..];
                    for child in children {
                        if let FileTreeNode::Directory { dfe, .. } = child {
                            if dfe.short_name == next_component {
                                return child.resolve_dir(true, remaining_components);
                            }
                        }
                    }
                    None // No matching directory found
                }
                else {
                    None
                }
            }
        }
    }
}
