use super::buffer::Buffer;
use std::collections::VecDeque;
use std::rc::Rc;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub struct Buffers {
    buffers: VecDeque<Buffer>,
}

impl Buffers {
    /// singleton of the home buffer
    pub fn home(syntax_set: Rc<SyntaxSet>, theme_set: Rc<ThemeSet>) -> Self {
        Self {
            buffers: VecDeque::from(vec![Buffer::home(syntax_set, theme_set)]),
        }
    }

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
