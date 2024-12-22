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
    pub name: String,
    pub index: usize,
    pub parent: usize,
    pub children: Vec<usize>,
    pub data: T,
}

#[derive(Clone)]
pub struct FoxTreeMap<T> {
    nodes: Vec<FoxTreeNode<T>>,
    name_to_index: FoxHashMap<String, usize>, // Name-to-index map for optional lookups
}

impl<T> FoxTreeMap<T> {
    pub fn new(root_data: T) -> Self {
        let root = FoxTreeNode {
            name: "root".to_string(),
            index: 0,
            parent: 0,
            children: Vec::new(),
            data: root_data,
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

    pub fn children(&self, index: usize) -> &[usize] {
        &self.nodes[index].children
    }

    pub fn node(&self, index: usize) -> &FoxTreeNode<T> {
        &self.nodes[index]
    }

    pub fn add_child(&mut self, parent: usize, name: &str, data: T) -> usize {
        let index = self.nodes.len();
        let node = FoxTreeNode {
            name: name.to_string(),
            index,
            parent,
            children: Vec::new(),
            data,
        };

        self.name_to_index.insert(name.to_string(), index);
        self.nodes[parent].children.push(index);
        self.nodes.push(node);
        index
    }

    /// Walks the tree and calls the callback on each node's data, immutably
    pub fn for_each<F>(&self, mut callback: F)
    where
        F: FnMut(usize, &T),
    {
        let mut visited = FoxHashSet::new();
        self.for_each_recursive(self.root(), &mut callback, &mut visited);
    }

    /// Internal helper for recursive traversal.
    fn for_each_recursive<F>(&self, index: usize, callback: &mut F, visited: &mut FoxHashSet<usize>)
    where
        F: FnMut(usize, &T),
    {
        if visited.contains(&index) {
            return; // Prevent cycles
        }
        visited.insert(index);

        let node = &self.nodes[index];
        callback(index, &node.data);

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
        println!("{}{}: {}", prefix, &node.name, display(&node.data));

        for &child in &node.children {
            self.debug_tree(child, indent + 2, display, visited);
        }
    }

    pub fn debug_with<F>(&self, f: &mut std::fmt::Formatter<'_>, display: &F) -> std::fmt::Result
    where
        F: Fn(&T) -> String,
    {
        let mut visited = FoxHashSet::default();
        self.debug_fmt_node_with(f, self.root(), 0, display, &mut visited)
    }

    pub fn debug_fmt_node_with<F>(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        index: usize,
        indent: usize,
        display: &F,
        visited: &mut FoxHashSet<usize>,
    ) -> std::fmt::Result
    where
        F: Fn(&T) -> String,
    {
        if visited.contains(&index) {
            writeln!(f, "{}(Cycle detected at node {})", " ".repeat(indent), index)?;
            return Ok(());
        }
        visited.insert(index);

        let node = &self.nodes[index];
        let prefix = " ".repeat(indent);
        writeln!(f, "{}{}: {}", prefix, node.name, display(&node.data))?;

        for &child in &node.children {
            self.debug_fmt_node_with(f, child, indent + 2, display, visited)?;
        }

        Ok(())
    }

    pub fn last_node(&self) -> (usize, usize) {
        let last = self.nodes.len().saturating_sub(1);
        (self.nodes[last].parent, last)
    }
}

pub trait FoxTree {
    type Data;

    fn tree_mut(&mut self) -> &mut FoxTreeMap<Self::Data>;
    fn tree(&self) -> &FoxTreeMap<Self::Data>;

    fn root(&self) -> usize {
        0
    }

    fn add_child<'a>(&'a mut self, parent: usize, name: &str, data: Self::Data) -> FoxTreeCursor<'a, Self::Data> {
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

    fn last_node(&mut self) -> FoxTreeCursor<Self::Data> {
        let last = self.tree().nodes.len() - 1;
        FoxTreeCursor {
            parent_index: self.tree().nodes[last].parent,
            current_index: last,
            tree: self.tree_mut(),
        }
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

    /// Add a child to the current node.
    pub fn add_child(mut self, name: &str, data: T) -> Self {
        let child_index = self.tree.add_child(self.current_index, name, data);
        self.parent_index = self.current_index;
        self.current_index = child_index;
        self
    }

    /// Add a sibling to the current node.
    pub fn add_sibling(mut self, name: &str, data: T) -> Self {
        let sibling_index = self.tree.add_child(self.parent_index, name, data);
        self.current_index = sibling_index;
        self
    }

    /// Return the parent node of this node.
    pub fn up(mut self) -> Self {
        let parent_node = self.tree.node(self.parent_index);
        self.parent_index = parent_node.parent;
        self.current_index = parent_node.index;
        self
    }

    /// Return the index of the current node.
    pub fn index(&self) -> usize {
        self.current_index
    }
}
