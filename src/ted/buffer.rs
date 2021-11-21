use super::Commands;
use crate::ted::format_space_chain;
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
    Normal,
    Insert,
}

const HELP: &str = r#"# Welcome to Ted

## NORMAL mode

- Press SPC q to quit from NORMAL mode.
- Use "h, j, k, l" keys to move your cursor around in normal mode.
- Edit text by entering INSERT mode with you "i" key.
- Press SPC to enter commands by chain.

## INSERT mode

- Press ESC to go back to normal mode.

## Commands

"#;

impl Default for Buffer {
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
    pub fn new(content: String, name: String) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.len() > 0 {
            lines
        } else {
            vec![String::default()]
        };
        Self {
            mode: Mode::Normal,
            lines,
            linum: 0,
            col: 0,
            changes: LinkedList::default(),
            name,
            file: None,
        }
    }

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
        self.mode = Mode::Normal;
        self.move_cursor_left(1);
    }

    pub fn move_cursor_left(&mut self, n: usize) {
        if self.col > n {
            self.move_cursor(self.linum, self.col - n);
        } else {
            self.move_cursor(self.linum, 0);
        }
    }

    pub fn move_cursor_right(&mut self, n: usize) {
        self.move_cursor(self.linum, self.col + n)
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

    pub fn move_cursor_bol(&mut self) {
        self.move_cursor(self.linum, 0)
    }

    pub fn move_cursor_eol(&mut self) {
        self.move_cursor(self.linum, self.get_eol())
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
        self.move_cursor_down(1);
        self.move_cursor_bol();
    }

    pub fn del_lines(&mut self, n: usize) -> Vec<String> {
        let u = self.lines.len().min(self.linum + n);
        let removed_lines: Vec<String> = self.lines.drain(self.linum..u).collect();
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
        removed_lines
    }

    pub fn clear_changes(&mut self) {
        self.changes.clear();
    }

    pub fn del_chars(&mut self, n: usize) -> String {
        let line = &mut self.lines[self.linum];
        if self.col < line.len() {
            let u = line.len().min(self.col + n);
            let chars: String = line.drain(self.col..u).collect();
            self.changes.push_back(Change::ModifiedLine(self.linum));
            return chars
        }
        String::default()
    }

    pub fn back_del_char(&mut self) {
        if self.col > 0 {
            self.move_cursor_left(1);
            self.del_chars(1);
        } else {
            self.del_lines(1);
            self.move_cursor_up(1);
            self.move_cursor_eol();
        }
    }

    fn get_eol(&self) -> usize {
        let n = self.get_current_line().len();
        match self.mode {
            Mode::Normal if n > 0 => n - 1,
            _ => n,
        }
    }
}
