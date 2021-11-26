use super::buffer::Buffer;
use std::collections::VecDeque;

pub struct Buffers {
    buffers: VecDeque<Buffer>,
}

impl Default for Buffers {
    fn default() -> Self {
        let mut buffers = VecDeque::default();
        buffers.push_back(Buffer::default());
        Buffers { buffers }
    }
}

impl Buffers {

    pub fn focused(&self) -> &Buffer {
        self.buffers.front().unwrap()
    }

    pub fn focused_mut(&mut self) -> &mut Buffer {
        self.buffers.front_mut().unwrap()
    }

    pub fn cycle_prev(&mut self) {
        if let Some(buffer) = self.buffers.pop_front() {
            self.buffers.push_back(buffer);
        }
    }

    pub fn cycle_next(&mut self) {
        if let Some(buffer) = self.buffers.pop_back() {
            self.buffers.push_front(buffer);
        }
    }

    pub fn new_buffer(&mut self, buffer: Buffer) {
        self.buffers.push_front(buffer);
    }

    pub fn len(&self) -> usize {
        self.buffers.len()
    }
}
