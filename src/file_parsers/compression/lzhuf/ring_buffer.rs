//! Ring buffer for LZ type compression windows

pub struct RingBuffer {
    buf: Vec<u8>,
    pos: usize,
    n: usize,
}

impl RingBuffer {
    pub fn create(n: usize) -> Self {
        Self {
            buf: vec![0; n],
            pos: 0,
            n,
        }
    }
    pub fn get_pos(&self, offset: i64) -> usize {
        (self.pos as i64 + offset).rem_euclid(self.n as i64) as usize
    }
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos % self.n;
    }
    /// use absolute index
    pub fn get_abs(&self, abs: usize) -> u8 {
        self.buf[abs % self.n]
    }
    /// use absolute index
    #[allow(dead_code)]
    pub fn set_abs(&mut self, abs: usize, val: u8) {
        self.buf[abs % self.n] = val;
    }
    pub fn get(&self, offset: i64) -> u8 {
        self.buf[(self.pos as i64 + offset).rem_euclid(self.n as i64) as usize]
    }
    pub fn set(&mut self, offset: i64, val: u8) {
        self.buf[(self.pos as i64 + offset).rem_euclid(self.n as i64) as usize] = val;
    }
    pub fn advance(&mut self) {
        self.pos = (self.pos + 1) % self.n;
    }
    pub fn retreat(&mut self) {
        self.pos = (self.pos - 1) % self.n;
    }
    /// Distance to another position, assuming it is behind us.
    /// Correctly handles positions that are "ahead" in memory order.
    pub fn distance_behind(&self, other: usize) -> usize {
        (self.pos as i64 - other as i64).rem_euclid(self.n as i64) as usize
    }
}

#[test]
fn offset() {
    let mut ring = RingBuffer::create(4);
    ring.set_pos(5);
    assert_eq!(ring.get_pos(0), 1);
    assert_eq!(ring.get_pos(4), 1);
    assert_eq!(ring.get_pos(3), 0);
    assert_eq!(ring.get_pos(-4), 1);
}

#[test]
fn distance() {
    // four positions 0 1 2 3
    // set position     ^       (wraps once)
    let mut ring = RingBuffer::create(4);
    ring.set_pos(5);
    assert_eq!(ring.get_pos(0), 1);
    assert_eq!(ring.distance_behind(0), 1);
    assert_eq!(ring.distance_behind(1), 0);
    assert_eq!(ring.distance_behind(3), 2);
}
