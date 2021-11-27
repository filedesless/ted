use super::Commands;
use crate::ted::format_space_chain;
use ropey::iter::Chars;
use ropey::Rope;
use std::fs::File;
use std::io;
use std::io::{Error, ErrorKind};
use std::ops::Range;
use std::path::Path;
use std::time::SystemTime;

#[derive(Clone)]
pub struct Buffer {
    pub name: String,
    pub mode: InputMode,
    pub edit_mode: EditMode,
    pub dirty: bool,
    file: Option<BackendFile>,
    content: Rope,
    cursor: usize, // 0..content.len_chars()
    selection: Option<usize>,
}

#[derive(Clone)]
pub struct BackendFile {
    path: String,
    modified: SystemTime,
}

#[derive(Copy, Clone)]
pub enum InputMode {
    Normal,
    Insert,
}

#[derive(Copy, Clone)]
pub enum EditMode {
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
    /// Basic in-memory buffer
    pub fn new(content: String, name: String) -> Self {
        Self {
            mode: InputMode::Normal,
            edit_mode: EditMode::Char,
            content: Rope::from(content),
            cursor: 0,
            name,
            file: None,
            selection: None,
            dirty: true,
        }
    }

    /// Buffer with a backend file to save to
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
            let output_file = File::create(file.path.clone())?;
            self.content.write_to(output_file)?;
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

    /// returns a non-empty line
    pub fn get_line(&self, linum: usize) -> Option<String> {
        if let Some(line) = self.content.get_line(linum) {
            if line.len_chars() > 0 {
                return Some(String::from(line));
            }
        }
        None
    }

    pub fn get_chars(&self) -> Chars {
        self.content.chars()
    }

    pub fn get_current_line(&self) -> Option<String> {
        let line_index = self.content.char_to_line(self.cursor);
        self.get_line(line_index)
    }

    pub fn set_current_line(&mut self, line: String) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let start = self.content.line_to_char(current_line_number);
        let end = self.content.line_to_char(current_line_number + 1);
        self.content.remove(start..end);
        self.content.insert(start, &line);
        self.cursor = self
            .cursor
            .min(self.content.line(current_line_number).len_chars());
    }

    /// returns (line_number, column_number)
    pub fn coord_from_pos(&self, pos: usize) -> (usize, usize) {
        let line_number = self.content.char_to_line(pos);
        let beginning_of_line = self.content.line_to_char(line_number);
        (line_number, pos.saturating_sub(beginning_of_line))
    }

    /// returns (cursor, column_number, line_number)
    pub fn get_cursor(&self) -> (usize, usize, usize) {
        let (x, y) = self.coord_from_pos(self.cursor);
        (self.cursor, x, y)
    }

    pub fn insert_char(&mut self, c: char) {
        self.content.insert_char(self.cursor, c);
        self.dirty = true;
        self.cursor += 1;
    }

    pub fn insert_mode(&mut self) {
        self.mode = InputMode::Insert;
    }

    pub fn normal_mode(&mut self) {
        match self.mode {
            InputMode::Insert => {
                self.mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    pub fn mark_selection(&mut self) {
        self.selection = Some(self.cursor);
        self.dirty = true;
    }

    pub fn remove_selection(&mut self) {
        self.selection = None;
        self.dirty = true;
    }

    pub fn get_selection_range(&self) -> Option<Range<usize>> {
        match self.edit_mode {
            EditMode::Char => self
                .selection
                .map(|selection| (selection.min(self.cursor)..selection.max(self.cursor))),
            EditMode::Line => self.selection.map(|selection| {
                let selected = self.content.char_to_line(selection);
                let current = self.content.char_to_line(self.cursor);
                let start = self.content.line_to_char(selected.min(current));
                let end = self.end_of_line(selected.max(current));
                start..end
            }),
        }
    }

    pub fn move_cursor_left(&mut self, n: usize) {
        let line_number = self.content.char_to_line(self.cursor);
        let beginning_of_line = self.content.line_to_char(line_number);
        if self.cursor == beginning_of_line {
            return;
        }

        match self.edit_mode {
            EditMode::Char => self.move_cursor(self.cursor.saturating_sub(n)),
            EditMode::Line => self.move_cursor(
                self.content
                    .line_to_char(self.content.char_to_line(self.cursor)),
            ),
        }
    }

    pub fn move_cursor_right(&mut self, n: usize) {
        let end_of_line = self.end_of_line(self.content.char_to_line(self.cursor));
        if self.cursor == end_of_line {
            return;
        }

        match self.edit_mode {
            EditMode::Char => self.move_cursor(self.cursor + n),
            EditMode::Line => self.move_cursor(
                self.content
                    .line_to_char(self.content.char_to_line(self.cursor) + 1)
                    .saturating_sub(1),
            ),
        }
    }

    /// will return last char position if line_number >= self.content.len_lines()
    fn end_of_line(&self, line_number: usize) -> usize {
        if let Some(line) = self.get_line(line_number) {
            let beginning_of_line = self.content.line_to_char(line_number);
            beginning_of_line + line.len().saturating_sub(1)
        } else {
            self.content.len_chars().saturating_sub(1)
        }
    }

    pub fn move_cursor_up(&mut self, n: usize) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let current_line_offset = self.cursor - self.content.line_to_char(current_line_number);
        let dest_line_number = current_line_number.saturating_sub(n);
        let dest_cursor = self.content.line_to_char(dest_line_number) + current_line_offset;
        self.move_cursor(dest_cursor.min(self.end_of_line(dest_line_number)));
    }

    pub fn move_cursor_down(&mut self, n: usize) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let current_line_offset = self.cursor - self.content.line_to_char(current_line_number);
        let dest_cursor = self.content.line_to_char(current_line_number + n) + current_line_offset;
        self.move_cursor(dest_cursor.min(self.end_of_line(current_line_number + n)));
    }

    pub fn move_cursor(&mut self, cursor: usize) {
        let new_position = cursor.min(self.content.len_chars().saturating_sub(1));
        if self.selection.is_some() {
            self.dirty = true;
        }
        self.cursor = new_position;
    }

    pub fn delete(&mut self, n: usize) {
        match self.edit_mode {
            EditMode::Line => self.delete_lines(n),
            EditMode::Char => self.delete_chars(n),
        }
    }

    pub fn delete_lines(&mut self, n: usize) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let start = self.content.line_to_char(current_line_number);
        let end_line_number = self.content.len_lines().min(current_line_number + n);
        let end = self.content.line_to_char(end_line_number);
        self.content.remove(start..end);
        self.move_cursor(start);
        self.dirty = true;
    }

    pub fn delete_chars(&mut self, n: usize) {
        let end = self.content.len_chars().min(self.cursor + n);
        self.content.remove(self.cursor..end);
        self.dirty = true;
    }

    pub fn back_delete_char(&mut self) {
        if self.cursor > 0 {
            self.move_cursor(self.cursor.saturating_sub(1));
            self.delete_chars(1);
        }
    }

    pub fn cycle_submode(&mut self) {
        self.dirty = true;
        self.edit_mode = match self.edit_mode {
            EditMode::Char => EditMode::Line,
            EditMode::Line => EditMode::Char,
        }
    }

    pub fn paste(&mut self, n: usize, text: &String) {
        for _ in 0..n {
            self.content.insert(self.cursor, text);
        }
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;
    #[test]
    fn end_of_line() {
        // empty line defaults to first char, even if there's none
        let buffer = Buffer::new(String::from(""), String::from(""));
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(String::from("\n"), String::from(""));
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(String::from("a\n"), String::from(""));
        assert_eq!(buffer.end_of_line(0), 1);
        let buffer = Buffer::new(String::from("a\nbb\n"), String::from(""));
        assert_eq!(buffer.end_of_line(1), 4);
        let buffer = Buffer::new(String::from("a\nbb"), String::from(""));
        assert_eq!(buffer.end_of_line(1), 3);
        // out of bound returns last pos
        let buffer = Buffer::new(String::from("a\nbb\n"), String::from(""));
        assert_eq!(buffer.end_of_line(2), 4);
        let buffer = Buffer::new(String::from("a\nbb\n"), String::from(""));
        assert_eq!(buffer.end_of_line(3), 4);
    }

    #[test]
    fn get_line() {
        let buffer = Buffer::new(String::from(""), String::from(""));
        assert_eq!(buffer.get_line(0).map(String::from), None);

        let buffer = Buffer::new(String::from("\n"), String::from(""));
        assert_eq!(
            buffer.get_line(0).map(String::from),
            Some(String::from("\n"))
        );
        assert_eq!(buffer.get_line(1).map(String::from), None);

        let buffer = Buffer::new(String::from("a\n\n"), String::from(""));
        assert_eq!(
            buffer.get_line(0).map(String::from),
            Some(String::from("a\n"))
        );
        assert_eq!(
            buffer.get_line(1).map(String::from),
            Some(String::from("\n"))
        );
        assert_eq!(buffer.get_line(2).map(String::from), None);
    }

    #[test]
    fn delete_line_out_of_bounds() {
        let mut buffer = Buffer::new(String::from(""), String::from(""));
        buffer.delete_lines(1000);
        assert_eq!(buffer.get_line(0), None);
    }

    #[test]
    fn delete_char_out_of_bounds() {
        let mut buffer = Buffer::new(String::from(""), String::from(""));
        buffer.delete_chars(1000);
    }
}
