//! Node Pool backed Binary Tree
//!
//! This is a binary tree, tuned for the task of indexing an LZSS dictionary.
//! The nodes must take unique values from 0..LEN-1, where LEN is the size of the node pool.
//! The value of the node is the index of its slot in the node pool.
//! Hence we can define a tree cursor simply by the index into the node pool, i.e., the
//! node value, index into the node pool, and cursor, are all one and the same.
//!
//! For application to LZSS we have the following.
//! * There can be a root node for each possible symbol, each one occupies a slot in the node pool.
//! * The size of the node pool corresponds to the size of the sliding window.
//! * The node value points to a slot in the sliding window.
//!
//! The implementation does a lot of error checking that an optimized code might not do.
//! This is a choice reflecting the expectation of small retro-files as data sets.

use num_derive::FromPrimitive;

/// Tree Errors
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("node is missing")]
    NodeMissing,
    #[error("node already exists")]
    NodeExists,
    #[error("out of range")]
    OutOfRange,
    #[error("there is no cursor")]
    NoCursor,
    #[error("tree connection is broken")]
    BrokenConnection,
    #[error("root node or broken connection")]
    BrokenConnectionOrRoot,
    #[error("non-root node or broken connection")]
    BrokenConnectionOrNotRoot,
}

#[derive(FromPrimitive, Clone, Copy)]
pub enum Side {
    Left = 0,
    Right = 1,
}

/// Node used to build a tree.
/// With this type of tree the values may also serve as node pointers.
#[derive(Clone)]
struct Node {
    /// value is also index into node pool
    val: usize,
    /// if this is a root, gives the value of the symbol
    symbol: Option<usize>,
    /// index to the parent
    up: Option<usize>,
    /// index to the children [left,right]
    down: [Option<usize>; 2],
}

pub struct Tree {
    /// one element for each symbol, value is index into the node pool
    roots: Vec<Option<usize>>,
    /// cursor as index into the node pool, can be None
    curs: Option<usize>,
    /// the node pool, one node for each unique value that is allowed
    pool: Vec<Node>,
}

impl Tree {
    /// The node values are from 0..len-1, the symbol values from 0..symbols-1.
    /// Each symbol may create its own root and tree within the node buffer.
    pub fn create(len: usize, symbols: usize) -> Self {
        let roots: Vec<Option<usize>> = vec![None; symbols];
        let mut pool: Vec<Node> = Vec::new();
        for i in 0..len {
            pool.push(Node {
                val: i,
                symbol: None,
                up: None,
                down: [None, None],
            });
        }
        Self {
            roots,
            curs: None,
            pool,
        }
    }
    fn chk_cursor(&self) -> Result<usize, Error> {
        match self.curs {
            None => Err(Error::NoCursor),
            Some(curs) => match curs < self.pool.len() {
                true => Ok(curs),
                false => Err(Error::OutOfRange),
            },
        }
    }
    pub fn get_cursor(&self) -> Option<usize> {
        self.curs
    }
    pub fn set_cursor(&mut self, curs: usize) -> Result<(), Error> {
        match curs < self.pool.len() {
            true => {
                self.curs = Some(curs);
                Ok(())
            }
            false => Err(Error::OutOfRange),
        }
    }
    pub fn set_cursor_to_root(&mut self, symbol: usize) -> Result<(), Error> {
        match self.roots[symbol] {
            None => Err(Error::NodeMissing),
            Some(v) => {
                self.curs = Some(v);
                Ok(())
            }
        }
    }
    #[allow(dead_code)]
    pub fn up(&mut self) -> Result<usize, Error> {
        match self.pool[self.chk_cursor()?].up {
            None => Err(Error::NodeMissing),
            Some(v) => {
                self.curs = Some(v);
                Ok(v)
            }
        }
    }
    pub fn down(&mut self, side: Side) -> Result<usize, Error> {
        match self.pool[self.chk_cursor()?].down[side as usize] {
            None => Err(Error::NodeMissing),
            Some(v) => {
                self.curs = Some(v);
                Ok(v)
            }
        }
    }
    /// Go down to the end on one side.  Cursor follows.
    pub fn terminus(&mut self, side: Side) -> Result<usize, Error> {
        let mut term = self.chk_cursor()?;
        while let Ok(curs) = self.down(side) {
            term = curs;
        }
        Ok(term)
    }
    /// Get the array of children, cursor does not move.
    pub fn get_down(&self) -> Result<[Option<usize>; 2], Error> {
        let curs = self.chk_cursor()?;
        Ok(self.pool[curs].down)
    }
    /// Get the parent and side (left or right) of the current cursor location.
    /// Cursor does not move.
    pub fn get_parent_and_side(&mut self) -> Result<(usize, Side), Error> {
        let curs = self.chk_cursor()?;
        if let Some(parent) = self.pool[curs].up {
            return match self.pool[parent].down {
                [Some(v), _] if v == curs => Ok((parent, Side::Left)),
                [_, Some(v)] if v == curs => Ok((parent, Side::Right)),
                _ => Err(Error::BrokenConnection),
            };
        }
        Err(Error::BrokenConnectionOrRoot)
    }
    /// Get the symbol and side (left or right) of the current cursor location.
    /// This is only used if we are on one of the roots of a multi-root tree.
    /// Cursor does not move.
    pub fn get_symbol(&mut self) -> Result<usize, Error> {
        let curs = self.chk_cursor()?;
        if let Some(symbol) = self.pool[curs].symbol {
            return match self.roots[symbol] {
                Some(v) if v == curs => Ok(symbol),
                _ => Err(Error::BrokenConnection),
            };
        }
        Err(Error::BrokenConnectionOrNotRoot)
    }
    pub fn is_root(&self) -> Result<bool, Error> {
        let curs = self.chk_cursor()?;
        Ok(self.pool[curs].up.is_none())
    }
    #[allow(dead_code)]
    pub fn is_leaf(&self) -> Result<bool, Error> {
        let curs = self.chk_cursor()?;
        Ok(self.pool[curs].down == [None, None])
    }
    pub fn is_free(&self, curs: usize) -> Result<bool, Error> {
        match (self.pool[curs].symbol, self.pool[curs].up, self.pool[curs].down) {
            // TODO: how should we define a free slot
            (None, None, _) => Ok(true),
            _ => Ok(false),
        }
    }
    /// Spawn a new node attaching to the cursor, cursor does not move.
    /// If the cursor is already linked downward an error is returned.
    /// If the target slot is already linked, the old links are overwritten.
    pub fn spawn(&mut self, val: usize, side: Side) -> Result<(), Error> {
        if val >= self.pool.len() {
            return Err(Error::OutOfRange);
        }
        let curs = self.chk_cursor()?;
        if self.pool[curs].down[side as usize].is_some() {
            eprintln!(
                "spawn: cannot overwrite {}",
                self.pool[curs].down[side as usize].unwrap()
            );
            return Err(Error::NodeExists);
        }
        self.pool[curs].down[side as usize] = Some(val);
        self.pool[val].up = Some(curs);
        self.pool[val].down = [None, None];
        Ok(())
    }
    /// This type of tree can have multiple roots or no roots.
    /// The root occupies a slot in the node pool, the slot must be free.
    pub fn spawn_root(&mut self, symbol: usize, curs: usize) -> Result<(), Error> {
        if symbol >= self.roots.len() || curs >= self.pool.len() {
            return Err(Error::OutOfRange);
        }
        if self.is_free(curs)? {
            self.roots[symbol] = Some(curs);
            self.pool[curs].symbol = Some(symbol);
            self.pool[curs].up = None;
            self.pool[curs].down = [None, None];
            return Ok(());
        }
        eprintln!("spawn_root: cannot overwrite {}", curs);
        Err(Error::NodeExists)
    }
    /// Drop nodes at and below the cursor.  On exit cursor moves up.
    /// If node is root, cursor becomes None.  This may be called recursively.
    pub fn drop(&mut self) -> Result<(), Error> {
        let curs = self.chk_cursor()?;
        let maybe_parent = self.pool[curs].up;
        let maybe_symbol = self.pool[curs].symbol;
        // recursively delete everything below
        if self.down(Side::Left).is_ok() {
            self.drop()?;
            self.set_cursor(curs)?;
        }
        if self.down(Side::Right).is_ok() {
            self.drop()?;
            self.set_cursor(curs)?;
        }
        // cut all links
        if let Some(parent) = maybe_parent {
            let (_, side) = self.get_parent_and_side()?;
            self.pool[parent].down[side as usize] = None;
            self.curs = Some(parent);
        }
        if let Some(symbol) = maybe_symbol {
            self.roots[symbol] = None;
            self.curs = None;
        }
        self.pool[curs].symbol = None;
        self.pool[curs].up = None;
        self.pool[curs].down = [None, None];
        Ok(())
    }
    /// Drop everything below the cursor on one side, OK if no branch to drop.
    pub fn drop_branch(&mut self, side: Side) -> Result<(), Error> {
        if self.down(side).is_ok() {
            self.drop()?;
        }
        Ok(())
    }
    /// Cut the links between this node and the one above.  Normally part of
    /// another operation (tree is left broken).
    pub fn cut_upward(&mut self) -> Result<(), Error> {
        let (parent, side) = self.get_parent_and_side()?;
        self.pool[parent].down[side as usize] = None;
        self.pool[self.curs.unwrap()].up = None;
        Ok(())
    }
    /// Cut the links between this node and one below.  Normally part of
    /// another operation (tree could be left broken).
    #[allow(dead_code)]
    pub fn cut_downward(&mut self, side: Side) -> Result<(), Error> {
        let curs: usize = self.chk_cursor()?;
        if let Some(son) = self.pool[curs].down[side as usize] {
            self.pool[curs].down[side as usize] = None;
            self.pool[son].up = None;
        }
        Ok(())
    }
    /// Move node and everything below to a new parent.
    /// If node has an existing parent the link is cut.
    /// If there is an existing branch and `force==true` it is dropped and replaced.
    /// If there is an existing branch and `force==false` an error is returned.
    /// Upon exit cursor points to the same node, only its parent has changed.
    /// This may free up slots in the node pool if `force==true`.
    pub fn move_node(&mut self, new_parent: usize, side: Side, force: bool) -> Result<(), Error> {
        let curs: usize = self.chk_cursor()?;
        match (self.pool[new_parent].down[side as usize], force) {
            (None, _) => {
                if self.pool[curs].up.is_some() {
                    self.cut_upward()?; // do first
                }
            }
            (Some(_), true) => {
                if self.pool[curs].up.is_some() {
                    self.cut_upward()?; // do first
                }
                self.set_cursor(new_parent)?;
                self.drop_branch(side)?;
                self.set_cursor(curs)?;
            }
            _ => {
                eprintln!(
                    "move: cannot overwrite {}",
                    self.pool[new_parent].down[side as usize].unwrap()
                );
                return Err(Error::NodeExists);
            }
        }
        self.pool[new_parent].down[side as usize] = Some(curs);
        self.pool[curs].up = Some(new_parent);
        Ok(())
    }
    /// Same as `move_node` except target node is a root
    pub fn move_node_to_root(&mut self, symbol: usize, force: bool) -> Result<(), Error> {
        let curs: usize = self.chk_cursor()?;
        match (self.roots[symbol], force) {
            (None, _) => {
                if self.pool[curs].up.is_some() {
                    self.cut_upward()?; // do first
                }
            }
            (Some(old_root), true) => {
                if self.pool[curs].up.is_some() {
                    self.cut_upward()?; // do first
                }
                self.set_cursor(old_root)?;
                self.drop()?;
                self.set_cursor(curs)?;
            }
            (Some(old_root), false) => {
                eprintln!("move: cannot overwrite root {}", old_root);
                return Err(Error::NodeExists);
            }
        }
        self.roots[symbol] = Some(curs);
        self.pool[curs].up = None;
        self.pool[curs].symbol = Some(symbol);
        Ok(())
    }
    /// Change the value of a node.  This frees one slot in the node pool and uses another.
    /// The cursor stays on the node, but its value has changed.
    /// If the new value is already used and `force==false` an error is returned,
    /// if `force==true` the former occupant of the slot is deleted.
    pub fn change_value(&mut self, new_val: usize, force: bool) -> Result<(), Error> {
        let old_val = self.chk_cursor()?;
        if new_val == old_val {
            return Ok(());
        }
        match (self.is_free(new_val)?, force) {
            (true, _) => {}
            (false, false) => {
                eprintln!("cannot change node value to {}", new_val);
                return Err(Error::NodeExists);
            }
            (false, true) => {
                self.set_cursor(new_val)?;
                self.drop()?;
                self.set_cursor(old_val)?;
            }
        }
        // update links pointing into old_val
        if let Some(symbol) = self.pool[old_val].symbol {
            self.roots[symbol] = Some(new_val);
        }
        if let Some(parent) = self.pool[old_val].up {
            let (_, side) = self.get_parent_and_side()?;
            self.pool[parent].down[side as usize] = Some(new_val);
        }
        if let Some(child) = self.pool[old_val].down[0] {
            self.pool[child].up = Some(new_val);
        }
        if let Some(child) = self.pool[old_val].down[1] {
            self.pool[child].up = Some(new_val);
        }
        // update links pointing out of old_val and new_val
        self.pool[new_val] = self.pool[old_val].clone();
        self.pool[new_val].val = new_val;
        self.pool[old_val] = Node {
            val: old_val,
            symbol: None,
            up: None,
            down: [None, None],
        };
        self.curs = Some(new_val);
        Ok(())
    }
}
