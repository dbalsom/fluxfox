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

//! A generic tree structure with fast key-value lookup (not collision safe!)

use crate::{FoxHashMap, FoxHashSet};

// A generic node in a FoxTreeMap
#[derive(Debug, Clone)]
pub struct FoxTreeNode<T> {
    pub name: Option<String>,
    pub data: T,
    pub children: Vec<usize>,
    pub index: usize,
}

pub struct FoxTreeMap<T> {
    nodes: Vec<FoxTreeNode<T>>,
    name_to_index: FoxHashMap<String, usize>, // Name-to-index map for optional lookups
}

impl<T> FoxTreeMap<T> {
    pub fn new(root_data: T) -> Self {
        let root = FoxTreeNode {
            name: Some("root".to_string()),
            data: root_data,
            children: Vec::new(),
            index: 0,
        };

        let mut name_to_index = FoxHashMap::default();
        name_to_index.insert("root".to_string(), 0);

        Self {
            nodes: vec![root],
            name_to_index,
        }
    }

    pub fn root(&self) -> usize {
        0 // Root is always index 0
    }

    pub fn add_child(&mut self, parent: usize, name: Option<&str>, data: T) -> usize {
        let index = self.nodes.len();
        let node = FoxTreeNode {
            name: name.map(|n| n.to_string()),
            data,
            children: Vec::new(),
            index,
        };

        if let Some(name) = name {
            self.name_to_index.insert(name.to_string(), index);
        }

        self.nodes[parent].children.push(index);
        self.nodes.push(node);
        index
    }

    /// Walks the tree and calls the callback on each node's data, immutably
    pub fn for_each<F>(&self, mut callback: F)
    where
        F: FnMut(&T),
    {
        let mut visited = FoxHashSet::new();
        self.for_each_recursive(self.root(), &mut callback, &mut visited);
    }

    /// Internal helper for recursive traversal.
    fn for_each_recursive<F>(&self, index: usize, callback: &mut F, visited: &mut FoxHashSet<usize>)
    where
        F: FnMut(&T),
    {
        if visited.contains(&index) {
            return; // Prevent cycles
        }
        visited.insert(index);

        let node = &self.nodes[index];
        callback(&node.data);

        for &child in &node.children {
            self.for_each_recursive(child, callback, visited);
        }
    }

    pub fn debug_tree<F>(&self, index: usize, indent: usize, display: &F, visited: &mut FoxHashSet<usize>)
    where
        F: Fn(&T) -> String,
    {
        if visited.contains(&index) {
            println!("{}(Cycle detected at node {})", " ".repeat(indent), index);
            return;
        }
        visited.insert(index);

        let node = &self.nodes[index];
        let prefix = " ".repeat(indent);
        let name_display = node.name.as_deref().unwrap_or("Unnamed");
        println!("{}{}: {}", prefix, name_display, display(&node.data));

        for &child in &node.children {
            self.debug_tree(child, indent + 2, display, visited);
        }
    }
}

pub trait FoxTree {
    type Data;

    fn tree_mut(&mut self) -> &mut FoxTreeMap<Self::Data>;
    fn tree(&self) -> &FoxTreeMap<Self::Data>;

    fn root(&self) -> usize {
        0
    }

    fn add_child<'a>(
        &'a mut self,
        parent: usize,
        name: Option<&str>,
        data: Self::Data,
    ) -> FoxTreeCursor<'a, Self::Data> {
        let child_index = self.tree_mut().add_child(parent, name, data);
        FoxTreeCursor {
            tree: self.tree_mut(),
            parent_index: parent,
            current_index: child_index,
        }
    }

    fn debug_tree(&self, display: impl Fn(&Self::Data) -> String) {
        let mut visited = FoxHashSet::new();
        self.tree().debug_tree(self.root(), 0, &display, &mut visited);
    }
}

// Cursor for chaining child and sibling additions
pub struct FoxTreeCursor<'a, T> {
    tree: &'a mut FoxTreeMap<T>,
    parent_index: usize,
    current_index: usize,
}

impl<'a, T: Default> FoxTreeCursor<'a, T> {
    pub fn new(tree: &'a mut FoxTreeMap<T>, parent_index: usize, current_index: usize) -> Self {
        Self {
            tree,
            parent_index,
            current_index,
        }
    }

    pub fn add_child(mut self, name: Option<&str>, data: T) -> Self {
        let child_index = self.tree.add_child(self.current_index, name, data);
        self.parent_index = self.current_index;
        self.current_index = child_index;
        self
    }

    pub fn add_sibling(mut self, name: Option<&str>, data: T) -> Self {
        let sibling_index = self.tree.add_child(self.parent_index, name, data);
        self.current_index = sibling_index;
        self
    }
}
