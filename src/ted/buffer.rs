use super::Commands;
use crate::ted::format_space_chain;
use std::cmp::Ordering::{Equal, Greater, Less};
use std::collections::LinkedList;
use std::io;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::time::SystemTime;

// TODO: rework into some kind of non empty doubly linked list with a cursor
#[derive(Clone)]
pub struct Buffer {
    pub mode: Mode,
    pub name: String,
    file: Option<BackendFile>,
    lines: Vec<String>,
    linum: usize, // within 0..lines.len()
    col: usize,   // within 0..=line.len()
    changes: LinkedList<Change>,
    selection: Option<(usize, usize)>,
}

#[derive(Clone)]
pub struct BackendFile {
    path: String,
    modified: SystemTime,
}

#[derive(Clone)]
pub enum Change {
    // ModifiedChar(usize, usize),
    // Indicates a line must be refreshed from the buffer
    ModifiedLine(usize), // within 0..lines.len()
    // Indicates a line must be removed from the screen
    DeletedLine(usize), // outside buffer boundaries
}

#[derive(Copy, Clone)]
pub enum Mode {
    Normal(SubMode),
    Insert,
}

#[derive(Copy, Clone)]
pub enum SubMode {
    Line,
    Char,
}

const HELP: &str = r#"# Welcome to Ted

## NORMAL mode

- Press SPC q to quit from NORMAL mode.
- Use "h, j, k, l" keys to move your cursor around in normal mode.
- Edit text by entering INSERT mode with your "i" key.
- Press SPC to enter commands by chain.

## INSERT mode

- Press ESC to go back to normal mode.

## Commands

"#;

impl Default for Buffer {
    // Home buffer with help
    fn default() -> Self {
        let mut message = String::from(HELP);
        for command in Commands::default().commands {
            let line = format!(
                "- {} ({}): {}\n",
                command.name,
                command
                    .chain
                    .as_ref()
                    .map(|chain| format_space_chain(chain))
                    .unwrap_or("unbound".to_string()),
                command.desc
            );
            message.push_str(&line);
        }
        Buffer::new(message, String::from("Buffer #1"))
    }
}

impl Buffer {
    // Basic in-memory buffer
    pub fn new(content: String, name: String) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.len() > 0 {
            lines
        } else {
            vec![String::default()]
        };
        Self {
            mode: Mode::Normal(SubMode::Char),
            lines,
            linum: 0,
            col: 0,
            changes: LinkedList::default(),
            name,
            file: None,
            selection: None,
        }
    }

    // Buffer with a backend file to save to
    pub fn from_file(path: &String) -> io::Result<Self> {
        let p = Path::new(&path);
        let name = if let Some(stem) = p.file_stem() {
            stem.to_string_lossy().to_string()
        } else {
            String::from("nameless file")
        };
        let epoch = SystemTime::UNIX_EPOCH;
        let (content, modified) = if p.exists() {
            let attr = std::fs::metadata(&path)?;
            (std::fs::read_to_string(&path)?, attr.modified()?)
        } else {
            (String::default(), epoch)
        };
        let mut buffer = Buffer::new(content, name);
        buffer.file = Some(BackendFile {
            path: path.to_string(),
            modified,
        });
        Ok(buffer)
    }

    pub fn overwrite_backend_file(&mut self) -> io::Result<()> {
        if let Some(file) = &mut self.file {
            let p = Path::new(&file.path);
            if let Ok(attr) = std::fs::metadata(p) {
                if let Ok(modified) = attr.modified() {
                    if file.modified < modified {
                        return Err(Error::new(ErrorKind::Other, "File modified since opened"));
                    }
                }
            }
            let content: Vec<String> = self.lines.iter().map(|s| s.clone()).collect();
            std::fs::write(file.path.clone(), content.join("\n"))?;
            file.modified = SystemTime::now();
            Ok(())
        } else {
            // TODO: ask for a file name to save
            Err(Error::new(ErrorKind::NotFound, "No backend file"))
        }
    }

    pub fn empty() -> Self {
        Buffer::new(String::default(), String::default())
    }

    pub fn get_changes(&self) -> &LinkedList<Change> {
        &self.changes
    }

    pub fn get_lines(&self) -> &Vec<String> {
        &self.lines
    }

    pub fn get_current_line(&self) -> &String {
        &self.lines[self.linum]
    }

    pub fn set_current_line(&mut self, line: String) {
        self.lines[self.linum] = line;
        self.col = self.col.min(self.get_eol());
    }

    pub fn get_line(&self, linum: usize) -> &String {
        &self.lines[linum]
    }

    pub fn get_cursor(&self) -> (usize, usize) {
        (self.linum, self.col)
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '\n' {
            self.new_line();
        } else {
            let line = &mut self.lines[self.linum];
            if self.col <= line.len() {
                line.insert(self.col, c);
                self.move_cursor_right(1);
            }
        }
    }

    pub fn insert_mode(&mut self) {
        self.mode = Mode::Insert;
    }

    pub fn normal_mode(&mut self) {
        match self.mode {
            Mode::Insert => {
                self.move_cursor_left(1);
                Mode::Normal(SubMode::Char);
            }
            _ => {}
        }
    }

    pub fn mark_selection(&mut self) {
        self.selection = Some((self.linum, self.col));
    }

    pub fn remove_selection(&mut self) {
        self.selection = None;
    }

    pub fn get_selection(&mut self) -> Vec<(usize, usize)> {
        let mut v = vec![];
        if let Some((linum, col)) = self.selection {
            let (marked, current) = ((linum, col), (self.linum, self.col));
            let ((lin1, col1), (lin2, col2)) = match linum.cmp(&self.linum) {
                Less => (marked, current),
                Greater => (current, marked),
                Equal => {
                    if col < self.col {
                        (marked, current)
                    } else {
                        (current, marked)
                    }
                }
            };
            for x in lin1..=lin2 {
                let line = &self.lines[x];
                if x == lin1 && x == lin2 {
                    for y in col1..(col2 + 1).min(line.len()) {
                        v.push((x, y));
                    }
                } else if x == lin1 {
                    for y in col1..line.len() {
                        v.push((x, y));
                    }
                } else if x == lin2 {
                    for y in 0..(col2 + 1).min(line.len()) {
                        v.push((x, y));
                    }
                } else {
                    for y in 0..line.len() {
                        v.push((x, y));
                    }
                }
            }
        }
        v
    }

    pub fn move_cursor_left(&mut self, n: usize) {
        let dst = if self.col > n { self.col - n } else { 0 };
        match self.mode {
            Mode::Insert => {}
            Mode::Normal(SubMode::Char) => self.move_cursor(self.linum, dst),
            Mode::Normal(SubMode::Line) => self.move_cursor(self.linum, 0),
        }
    }

    pub fn move_cursor_right(&mut self, n: usize) {
        match self.mode {
            Mode::Insert => {}
            Mode::Normal(SubMode::Char) => self.move_cursor(self.linum, self.col + n),
            Mode::Normal(SubMode::Line) => self.move_cursor(self.linum, self.get_eol()),
        }
    }

    pub fn move_cursor_up(&mut self, n: usize) {
        if self.linum >= n {
            self.move_cursor(self.linum - n, self.col);
        } else {
            self.move_cursor(0, self.col);
        }
    }

    pub fn move_cursor_down(&mut self, n: usize) {
        self.move_cursor(self.linum + n, self.col)
    }

    pub fn move_cursor(&mut self, linum: usize, col: usize) {
        if self.lines.len() > 0 {
            self.linum = linum.min(self.lines.len() - 1);
        }

        self.col = col.min(self.get_eol());
    }

    pub fn new_line(&mut self) {
        let (old, new) = self.lines[self.linum].split_at(self.col);
        let s = String::from(new);
        self.lines[self.linum] = String::from(old);
        self.lines.insert(self.linum + 1, s);
        for i in self.linum..self.lines.len() {
            self.changes.push_back(Change::ModifiedLine(i))
        }
        self.move_cursor(self.linum + 1, 0);
    }

    pub fn clear_changes(&mut self) {
        self.changes.clear();
    }

    pub fn delete(&mut self, n: usize) -> String {
        match self.mode {
            Mode::Insert => String::default(),
            Mode::Normal(SubMode::Line) => self.delete_lines(n),
            Mode::Normal(SubMode::Char) => self.delete_chars(n),
        }
    }

    pub fn delete_lines(&mut self, n: usize) -> String {
        let u = self.lines.len().min(self.linum + n);
        let removed_lines: Vec<String> = self
            .lines
            .drain(self.linum..u)
            .map(|line| format!("{}\n", line))
            .collect();
        if self.lines.len() > 0 {
            self.linum = self.linum.min(self.lines.len() - 1);
        } else {
            self.linum = 0;
            self.lines = vec![String::default()];
        }
        for i in self.linum..self.lines.len() {
            self.changes.push_back(Change::ModifiedLine(i))
        }
        for i in self.lines.len()..self.lines.len() + removed_lines.len() {
            self.changes.push_back(Change::DeletedLine(i));
        }
        self.col = self.col.min(self.get_eol());
        removed_lines.join("")
    }

    // TODO: handle crossing line boundaries
    pub fn delete_chars(&mut self, n: usize) -> String {
        let line = &mut self.lines[self.linum];
        if self.col < line.len() {
            let u = line.len().min(self.col + n);
            let chars: String = line.drain(self.col..u).collect();
            self.changes.push_back(Change::ModifiedLine(self.linum));
            return chars;
        }
        String::default()
    }

    pub fn back_delete_char(&mut self) {
        if self.col > 0 {
            self.move_cursor_left(1);
            self.delete_chars(1);
        } else {
            self.delete_lines(1);
            self.move_cursor(self.linum, self.get_eol());
        }
    }

    pub fn cycle_submode(&mut self) {
        self.mode = match self.mode {
            Mode::Insert => Mode::Insert,
            Mode::Normal(SubMode::Char) => Mode::Normal(SubMode::Line),
            Mode::Normal(SubMode::Line) => Mode::Normal(SubMode::Char),
        }
    }

    pub fn paste(&mut self, n: usize, text: &String) {
        let mode = self.mode;
        self.mode = Mode::Insert;
        let (linum, col) = self.get_cursor();
        for _ in 0..n {
            for c in text.chars() {
                self.insert_char(c);
            }
        }
        self.linum = linum;
        self.col = col;
        self.mode = mode;
    }

    fn get_eol(&self) -> usize {
        let n = self.get_current_line().len();
        match self.mode {
            Mode::Normal(_) if n > 0 => n - 1,
            _ => n,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    pub fn line_reverse_offset() {
        let line = String::from("Hello world");
        let i = 5;
        let mut chars = line.chars();
        assert_eq!(chars.nth(i).unwrap(), ' ');
        let mut back = line.chars().rev().skip(line.len() - i - 1);
        assert_eq!(back.nth(0).unwrap(), ' ');
    }
}
