use super::buffer::Buffer;
use std::collections::LinkedList;

pub struct RingBuffer {
    buffers: LinkedList<Buffer>,
}

impl Default for RingBuffer {
    fn default() -> Self {
        let mut buffers = LinkedList::default();
        buffers.push_back(Buffer::default());
        Self { buffers }
    }
}

impl RingBuffer {
    pub fn focused(&mut self) -> &mut Buffer {
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
