use super::Commands;
use crate::ted::cached_highlighter::CachedHighlighter;
use crate::ted::format_space_chain;
use ropey::iter::Chars;
use ropey::Rope;
use std::fs::File;
use std::io;
use std::io::{Error, ErrorKind};
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::time::SystemTime;
use syntect::highlighting::Theme;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxReference;
use syntect::parsing::SyntaxSet;

const DEFAULT_THEME: &str = "base16-eighties.dark";

pub struct Buffer {
    pub name: String,
    pub mode: InputMode,
    pub edit_mode: EditMode,
    pub dirty: bool,
    window: Range<usize>,
    file: Option<BackendFile>,
    content: Rope,
    cursor: usize, // 0..content.len_chars()
    selection: Option<usize>,
    syntax_set: Rc<SyntaxSet>,
    theme_set: Rc<ThemeSet>,
    syntax: SyntaxReference,
    theme: Theme,
    cached_highlighter: CachedHighlighter,
}

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

impl Buffer {
    /// Basic in-memory buffer
    pub fn new(
        content: String,
        name: String,
        syntax_set: Rc<SyntaxSet>,
        theme_set: Rc<ThemeSet>,
    ) -> Self {
        let theme = theme_set.themes[DEFAULT_THEME].clone();
        let syntax = syntax_set.find_syntax_plain_text();
        Self {
            mode: InputMode::Normal,
            edit_mode: EditMode::Char,
            content: Rope::from(content),
            cursor: 0,
            name,
            file: None,
            selection: None,
            dirty: true,
            window: 0..1,
            syntax_set: syntax_set.clone(),
            theme_set: theme_set.clone(),
            syntax: syntax.clone(),
            theme: theme.clone(),
            cached_highlighter: CachedHighlighter::new(syntax.clone(), syntax_set, theme),
        }
    }

    /// Home buffer with help
    pub fn home(syntax_set: Rc<SyntaxSet>, theme_set: Rc<ThemeSet>) -> Self {
        let mut message = String::from(HELP);
        for command in Commands::default().commands {
            let line = format!(
                "- {} ({}): {}\n",
                command.name,
                command
                    .chain
                    .as_ref()
                    .map(|chain| format_space_chain(chain))
                    .unwrap_or_else(|| "unbound".to_string()),
                command.desc
            );
            message.push_str(&line);
        }
        let mut buffer = Buffer::new(message, String::from("Buffer #1"), syntax_set, theme_set);
        buffer.set_language(&"Markdown".to_string());
        buffer
    }

    /// Buffer with a backend file to save to
    pub fn from_file(
        path: &str,
        syntax_set: Rc<SyntaxSet>,
        theme_set: Rc<ThemeSet>,
    ) -> io::Result<Self> {
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
        let mut buffer = Buffer::new(content, name, syntax_set.clone(), theme_set);
        buffer.file = Some(BackendFile {
            path: path.to_string(),
            modified,
        });
        let from_ext = buffer
            .file
            .as_ref()
            .and_then(|file| Path::new(&file.path).extension())
            .and_then(|e| e.to_str())
            .and_then(|extension| syntax_set.find_syntax_by_extension(extension));
        let from_line = buffer
            .content
            .get_line(0)
            .and_then(|line| syntax_set.find_syntax_by_first_line(&line.to_string()));
        if let Some(syntax) = from_line.or(from_ext) {
            buffer.syntax = syntax.clone();
            buffer.cached_highlighter = CachedHighlighter::new(
                syntax.clone(),
                buffer.syntax_set.clone(),
                buffer.theme.clone(),
            );
        }
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

    pub fn empty(syntax_set: Rc<SyntaxSet>, theme_set: Rc<ThemeSet>) -> Self {
        Buffer::new(String::default(), String::default(), syntax_set, theme_set)
    }

    /// returns a non-empty line
    pub fn get_line(&self, line_number: usize) -> Option<String> {
        if let Some(line) = self.content.get_line(line_number) {
            if line.len_chars() > 0 {
                return Some(String::from(line));
            }
        }
        None
    }

    pub fn get_syntax(&self) -> &SyntaxReference {
        &self.syntax
    }

    pub fn set_language(&mut self, language: &str) -> bool {
        if let Some(syntax) = self.syntax_set.find_syntax_by_name(language) {
            self.syntax = syntax.clone();
            self.cached_highlighter = CachedHighlighter::new(
                self.syntax.clone(),
                self.syntax_set.clone(),
                self.theme.clone(),
            );
            self.dirty = true;
            return true;
        }
        false
    }

    pub fn get_theme(&self) -> String {
        self.theme
            .name
            .as_ref()
            .unwrap_or(&DEFAULT_THEME.to_string())
            .to_string()
    }

    pub fn set_theme(&mut self, name: &str) -> bool {
        if let Some(theme) = self.theme_set.themes.get(name) {
            self.theme = theme.clone();
            self.cached_highlighter = CachedHighlighter::new(
                self.syntax.clone(),
                self.syntax_set.clone(),
                self.theme.clone(),
            );
            self.dirty = true;
            return true;
        }
        false
    }

    /// returns highlighted lines within the view range
    pub fn get_highlighted_lines(&mut self) -> Vec<(String, usize)> {
        self.cached_highlighter
            .get_highlighted_lines(self.content.clone(), self.window.clone())
    }

    pub fn resize_window(&mut self, height: usize) {
        self.window.end = self.window.start + height;
        if self.content.char_to_line(self.cursor) >= self.window.end {
            self.cursor = self.end_of_line(self.window.end);
        }
    }

    /// returns chars from the buffer and the offset of the first
    pub fn get_chars(&self) -> Option<(Chars, usize)> {
        if self.window.start <= self.content.len_lines() {
            let bol = self.content.line_to_char(self.window.start);
            return Some((self.content.chars_at(bol), bol));
        }
        None
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

    /// returns the [first_line_number, last_line_number) within view
    pub fn get_window(&self) -> &Range<usize> {
        &self.window
    }

    /// returns (line_number, column_number) within self.window
    pub fn coord_from_pos(&self, pos: usize) -> (usize, usize) {
        let line_number = self.content.char_to_line(pos);
        let beginning_of_line = self.content.line_to_char(line_number);
        (line_number, pos.saturating_sub(beginning_of_line))
    }

    /// returns (cursor, line_number, column_number)
    pub fn get_cursor(&self) -> (usize, usize, usize) {
        let (line_number, column_number) = self.coord_from_pos(self.cursor);
        (self.cursor, line_number, column_number)
    }

    pub fn insert_char(&mut self, c: char) {
        self.content.insert_char(self.cursor, c);
        self.dirty = true;
        let line_number = self.content.char_to_line(self.cursor);
        self.cached_highlighter.invalidate_from(line_number);
        // TODO: refac to use self.move_cursor without breaking minibuffer
        self.cursor += 1;
    }

    pub fn insert_mode(&mut self) {
        self.mode = InputMode::Insert;
    }

    pub fn normal_mode(&mut self) {
        if let InputMode::Insert = self.mode {
            self.mode = InputMode::Normal;
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

    pub fn move_cursor_bol(&mut self) {
        let current_line = self.content.char_to_line(self.cursor);
        self.move_cursor(self.content.line_to_char(current_line));
    }

    pub fn move_cursor_eol(&mut self) {
        let current_line = self.content.char_to_line(self.cursor);
        self.move_cursor(self.end_of_line(current_line));
    }

    pub fn move_cursor_left(&mut self, n: usize) {
        let line_number = self.content.char_to_line(self.cursor);
        let beginning_of_line = self.content.line_to_char(line_number);
        if self.cursor == beginning_of_line {
            return;
        }

        self.move_cursor(self.cursor.saturating_sub(n));
    }

    pub fn move_cursor_right(&mut self, n: usize) {
        let end_of_line = self.end_of_line(self.content.char_to_line(self.cursor));
        if self.cursor == end_of_line {
            return;
        }

        self.move_cursor(self.cursor + n);
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
        let dest_line_number = self.content.len_lines().min(current_line_number + n);
        let dest_cursor = self.content.line_to_char(dest_line_number) + current_line_offset;
        self.move_cursor(dest_cursor.min(self.end_of_line(dest_line_number)));
    }

    pub fn move_cursor(&mut self, cursor: usize) {
        if self.selection.is_some() {
            self.dirty = true;
        }
        let dest_line_number = self.content.char_to_line(cursor);
        if dest_line_number < self.window.start {
            let offset = self.window.start - dest_line_number; // at least 1
            self.window = self.window.start - offset..self.window.end - offset;
            self.dirty = true;
        }
        if dest_line_number >= self.window.end {
            let offset = dest_line_number - self.window.end + 1; // at least 1
            self.window = (self.window.start + offset)..(self.window.end + offset);
            self.dirty = true;
        }
        self.cursor = cursor.min(self.content.len_chars().saturating_sub(1));
    }

    pub fn page_up(&mut self, n: usize) {
        let height = self.window.end - self.window.start;
        self.move_cursor_up((height / 2) * n);
    }

    pub fn page_down(&mut self, n: usize) {
        let height = self.window.end - self.window.start;
        self.move_cursor_down((height / 2) * n);
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
        self.cached_highlighter.invalidate_from(current_line_number);
    }

    pub fn delete_chars(&mut self, n: usize) {
        let end = self.content.len_chars().min(self.cursor + n);
        self.content.remove(self.cursor..end);
        self.dirty = true;
        // self.highlighted_lines = None;
        let line_number = self.content.char_to_line(self.cursor);
        self.cached_highlighter.invalidate_from(line_number);
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

    pub fn paste(&mut self, n: usize, text: &str) {
        for _ in 0..n {
            self.content.insert(self.cursor, text);
        }
        self.dirty = true;
        let line_number = self.content.char_to_line(self.cursor);
        self.cached_highlighter.invalidate_from(line_number);
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;
    use std::rc::Rc;
    use syntect::highlighting::ThemeSet;
    use syntect::parsing::SyntaxSet;

    #[test]
    fn end_of_line() {
        let ss = Rc::new(SyntaxSet::load_defaults_newlines());
        let ts = Rc::new(ThemeSet::load_defaults());
        // empty line defaults to first char, even if there's none
        let buffer = Buffer::new(String::from(""), String::from(""), ss.clone(), ts.clone());
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(String::from("\n"), String::from(""), ss.clone(), ts.clone());
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(
            String::from("a\n"),
            String::from(""),
            ss.clone(),
            ts.clone(),
        );
        assert_eq!(buffer.end_of_line(0), 1);
        let buffer = Buffer::new(
            String::from("a\nbb\n"),
            String::from(""),
            ss.clone(),
            ts.clone(),
        );
        assert_eq!(buffer.end_of_line(1), 4);
        let buffer = Buffer::new(
            String::from("a\nbb"),
            String::from(""),
            ss.clone(),
            ts.clone(),
        );
        assert_eq!(buffer.end_of_line(1), 3);
        // out of bound returns last pos
        let buffer = Buffer::new(
            String::from("a\nbb\n"),
            String::from(""),
            ss.clone(),
            ts.clone(),
        );
        assert_eq!(buffer.end_of_line(2), 4);
        let buffer = Buffer::new(
            String::from("a\nbb\n"),
            String::from(""),
            ss.clone(),
            ts.clone(),
        );
        assert_eq!(buffer.end_of_line(3), 4);
    }

    #[test]
    fn get_line() {
        let ss = Rc::new(SyntaxSet::load_defaults_newlines());
        let ts = Rc::new(ThemeSet::load_defaults());

        let buffer = Buffer::new(String::from(""), String::from(""), ss.clone(), ts.clone());
        assert_eq!(buffer.get_line(0).map(String::from), None);

        let buffer = Buffer::new(String::from("\n"), String::from(""), ss.clone(), ts.clone());
        assert_eq!(
            buffer.get_line(0).map(String::from),
            Some(String::from("\n"))
        );
        assert_eq!(buffer.get_line(1).map(String::from), None);

        let buffer = Buffer::new(
            String::from("a\n\n"),
            String::from(""),
            ss.clone(),
            ts.clone(),
        );
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
        let ss = Rc::new(SyntaxSet::load_defaults_newlines());
        let ts = Rc::new(ThemeSet::load_defaults());
        let mut buffer = Buffer::new(String::from(""), String::from(""), ss.clone(), ts.clone());
        buffer.delete_lines(1000);
        assert_eq!(buffer.get_line(0), None);
    }

    #[test]
    fn delete_char_out_of_bounds() {
        let ss = Rc::new(SyntaxSet::load_defaults_newlines());
        let ts = Rc::new(ThemeSet::load_defaults());
        let mut buffer = Buffer::new(String::from(""), String::from(""), ss.clone(), ts.clone());
        buffer.delete_chars(1000);
    }
}
