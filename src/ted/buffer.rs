use super::Commands;
use crate::ted::cached_highlighter::CachedHighlighter;
use crate::ted::format_space_chain;
use crate::ted::Config;
use ropey::Rope;
use std::fs::File;
use std::io;
use std::io::{Error, ErrorKind};
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::time::SystemTime;

const DEFAULT_THEME: &str = "ted";

pub struct Buffer {
    pub name: String,
    pub mode: InputMode,
    window: Range<usize>,
    file: Option<BackendFile>,
    content: Rope,
    cursor: usize, // 0..content.len_chars()
    last_col: usize,
    selection: Option<usize>,
    config: Rc<Config>,
    highlighter: Option<CachedHighlighter>,
}

pub struct BackendFile {
    path: String,
    modified: SystemTime,
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum InputMode {
    Normal,
    Insert,
}

type HighlightedLine = (String, Vec<(syntect::highlighting::Style, Range<usize>)>);
pub enum Lines {
    Highlighted(Vec<HighlightedLine>),
    Plain(Vec<String>),
}

const HELP: &str = r#"# Welcome to Ted

## NORMAL mode

In this mode keystrokes have a special meaning, mostly mimicking vim.

- `SPC q` to quit ted
- `SPC` to enter commands by chain

### Moving the cursor

- `h, j, k, l` to move your cursor around in normal mode
- `J, K` to move a page up or down
- `H, L` to move beginning or end of line

### Enter INSERT mode

- `i, I` to insert under cursor or at beginning of line
- `a, A` to append under cursor or at end of line
- `o, O` to append newline under or above current line

### Edit the buffer

- `d, D` to delete the character or line under cursor

## INSERT mode

In this mode keystrokes are inserted in the buffer, press `ESC` to go back to normal mode

## SPACE chains

Enter chains starting with `SPC` to run the following commands

"#;

impl Buffer {
    /// Basic in-memory buffer
    pub fn new(content: String, name: String, config: Rc<Config>) -> Self {
        Self {
            mode: InputMode::Normal,
            content: Rope::from(content),
            highlighter: None,
            config,
            cursor: 0,
            last_col: 0,
            name,
            file: None,
            selection: None,
            window: 0..1,
        }
    }

    /// Home buffer with help
    pub fn home(config: Rc<Config>) -> Self {
        let mut message = String::from(HELP);
        for command in Commands::default().commands {
            let line = format!(
                "- `{}` ({}): {}\n",
                command
                    .chain
                    .as_ref()
                    .map(|chain| format_space_chain(chain))
                    .unwrap_or_else(|| "unbound".to_string()),
                command.name,
                command.desc
            );
            message.push_str(&line);
        }
        let mut buffer = Buffer::new(message, String::from("Buffer #1"), config);
        buffer.set_language(&"Markdown".to_string());
        buffer
    }

    /// Buffer with a backend file to save to
    pub fn from_file(path: &str, config: Rc<Config>) -> io::Result<Self> {
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
        let mut buffer = Buffer::new(content, name, config.clone());
        buffer.file = Some(BackendFile {
            path: path.to_string(),
            modified,
        });
        let from_ext = buffer
            .file
            .as_ref()
            .and_then(|file| Path::new(&file.path).extension())
            .and_then(|e| e.to_str())
            .and_then(|extension| config.syntax_set.find_syntax_by_extension(extension));
        let from_line = buffer.content.get_line(0).and_then(|line| {
            config
                .syntax_set
                .find_syntax_by_first_line(&line.to_string())
        });
        if let Some(syntax) = from_line.or(from_ext).cloned() {
            let theme = config
                .theme_set
                .themes
                .get(DEFAULT_THEME)
                .cloned()
                .unwrap_or_default();
            buffer.highlighter = Some(CachedHighlighter::new(syntax, theme, config));
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

    /// returns a non-empty line
    pub fn get_line(&self, line_number: usize) -> Option<String> {
        if let Some(line) = self.content.get_line(line_number) {
            if line.len_chars() > 0 {
                return Some(String::from(line));
            }
        }
        None
    }

    pub fn set_language(&mut self, language: &str) -> bool {
        if let Some(syntax) = self.config.syntax_set.find_syntax_by_name(language) {
            self.highlighter = Some(CachedHighlighter::new(
                syntax.clone(),
                self.config
                    .theme_set
                    .themes
                    .get(DEFAULT_THEME)
                    .cloned()
                    .unwrap_or_default(),
                self.config.clone(),
            ));
            return true;
        }
        false
    }

    pub fn set_theme(&mut self, name: &str) -> bool {
        if let Some(cached) = self.highlighter.as_mut() {
            if let Some(theme) = self.config.theme_set.themes.get(name).cloned() {
                cached.set_theme(theme);
                return true;
            }
        }
        false
    }

    /// returns highlighted lines within the view range
    pub fn get_visible_lines(&mut self) -> Lines {
        if let Some(cached) = self.highlighter.as_mut() {
            Lines::Highlighted(
                cached.get_highlighted_lines(self.content.clone(), self.window.clone()),
            )
        } else {
            Lines::Plain(
                self.content
                    .get_lines_at(self.window.start)
                    .map(|lines| lines.take(self.window.len()).map(String::from).collect())
                    .unwrap_or_else(Vec::new),
            )
        }
    }

    pub fn resize_window(&mut self, height: usize) {
        self.window.end = self.window.start + height;
        if self.content.char_to_line(self.cursor) >= self.window.end {
            self.cursor = self.end_of_line(self.window.end);
        }
    }

    /// returns the [first_line_number, last_line_number) within view
    pub fn get_window(&self) -> &Range<usize> {
        &self.window
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }

    pub fn get_highlighter(&self) -> &Option<CachedHighlighter> {
        &self.highlighter
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
        let line_number = self.content.char_to_line(self.cursor);
        if let Some(cached) = self.highlighter.as_mut() {
            cached.invalidate_from(line_number)
        }
        self.move_cursor(self.cursor + 1);
    }

    pub fn prepend_newline(&mut self) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let bol = self.content.line_to_char(current_line_number);
        self.content.insert_char(bol, '\n');
        if let Some(cached) = self.highlighter.as_mut() {
            cached.invalidate_from(current_line_number)
        }
        if self.cursor != bol {
            self.move_cursor_up(1);
        }
    }

    pub fn append_newline(&mut self) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let eol = self.end_of_line(current_line_number);
        self.content.insert_char(eol, '\n');
        if let Some(cached) = self.highlighter.as_mut() {
            cached.invalidate_from(current_line_number)
        }
        self.move_cursor_down(1);
    }

    pub fn insert_mode(&mut self) {
        self.mode = InputMode::Insert;
    }

    pub fn normal_mode(&mut self) {
        if let InputMode::Insert = self.mode {
            self.mode = InputMode::Normal;
            self.move_cursor(
                self.cursor
                    .min(self.end_of_line(self.content.char_to_line(self.cursor))),
            );
        }
    }

    pub fn mark_selection(&mut self) {
        self.selection = Some(self.cursor);
    }

    pub fn remove_selection(&mut self) {
        self.selection = None;
    }

    pub fn get_selection_range(&self) -> Option<Range<usize>> {
        self.selection
            .map(|selection| (selection.min(self.cursor)..selection.max(self.cursor)))
    }

    pub fn move_cursor_bol(&mut self) {
        let current_line = self.content.char_to_line(self.cursor);
        let dest_cursor = self.content.line_to_char(current_line);
        if dest_cursor != self.cursor {
            self.move_cursor(dest_cursor);
            self.last_col = self.cursor - self.content.line_to_char(current_line);
        }
    }

    pub fn move_cursor_eol(&mut self) {
        let current_line = self.content.char_to_line(self.cursor);
        let dest_cursor = self.end_of_line(current_line);
        if dest_cursor != self.cursor {
            self.move_cursor(dest_cursor);
            self.last_col = self.cursor - self.content.line_to_char(current_line);
        }
    }

    pub fn move_cursor_left(&mut self, n: usize) {
        let line_number = self.content.char_to_line(self.cursor);

        let dest_cursor = self
            .content
            .line_to_char(line_number)
            .max(self.cursor.saturating_sub(n));
        if dest_cursor != self.cursor {
            self.move_cursor(dest_cursor);
            self.last_col = self.cursor - self.content.line_to_char(line_number);
        }
    }

    pub fn move_cursor_right(&mut self, n: usize) {
        let line_number = self.content.char_to_line(self.cursor);
        let dest_cursor = self.end_of_line(line_number).min(self.cursor + n);
        if dest_cursor != self.cursor {
            self.move_cursor(dest_cursor);
            self.last_col = self.cursor - self.content.line_to_char(line_number);
        }
    }

    /// will return last char position if line_number >= self.content.len_lines()
    fn end_of_line(&self, line_number: usize) -> usize {
        let off_one = (self.mode != InputMode::Insert) as usize;
        if let Some(line) = self.get_line(line_number) {
            let beginning_of_line = self.content.line_to_char(line_number);
            let trimmed = line.replace("\n", "");
            beginning_of_line + trimmed.len().saturating_sub(off_one)
        } else {
            self.content.len_chars().saturating_sub(1 + off_one)
        }
    }

    pub fn move_cursor_up(&mut self, n: usize) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let current_line_offset = self.cursor - self.content.line_to_char(current_line_number);
        let dest_line_number = current_line_number.saturating_sub(n);
        let dest_cursor =
            self.content.line_to_char(dest_line_number) + current_line_offset.max(self.last_col);
        self.move_cursor(dest_cursor.min(self.end_of_line(dest_line_number)));
    }

    pub fn move_cursor_down(&mut self, n: usize) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let current_line_offset = self.cursor - self.content.line_to_char(current_line_number);
        let dest_line_number = self.content.len_lines().min(current_line_number + n);
        // find the furthest line that's non-empty
        for line_number in (current_line_number..=dest_line_number).rev() {
            if self.get_line(line_number).is_some() {
                let dest_cursor =
                    self.content.line_to_char(line_number) + current_line_offset.max(self.last_col);
                self.move_cursor(dest_cursor.min(self.end_of_line(line_number)));
                return;
            }
        }
    }

    pub fn move_cursor(&mut self, cursor: usize) {
        let cursor = cursor.clamp(0, self.content.len_chars().saturating_sub(1));
        let dest_line_number = self.content.char_to_line(cursor);
        if dest_line_number < self.window.start {
            let offset = self.window.start - dest_line_number; // at least 1
            self.window = self.window.start - offset..self.window.end - offset;
        }
        if dest_line_number >= self.window.end {
            let offset = dest_line_number - self.window.end + 1; // at least 1
            self.window = (self.window.start + offset)..(self.window.end + offset);
        }
        self.cursor = cursor;
    }

    pub fn page_up(&mut self, n: usize) {
        let height = self.window.end - self.window.start;
        self.move_cursor_up((height / 2) * n);
    }

    pub fn page_down(&mut self, n: usize) {
        let height = self.window.end - self.window.start;
        self.move_cursor_down((height / 2) * n);
    }

    pub fn delete_lines(&mut self, n: usize) {
        let current_line_number = self.content.char_to_line(self.cursor);
        let start = self.content.line_to_char(current_line_number);
        let end_line_number = self.content.len_lines().min(current_line_number + n);
        let end = self.content.line_to_char(end_line_number);
        self.content.remove(start..end);
        let last_line_number = self.content.len_lines().saturating_sub(2);
        let line_number = current_line_number.min(last_line_number);
        self.move_cursor(
            self.end_of_line(line_number)
                .min(self.content.line_to_char(line_number) + self.last_col),
        );
        if let Some(cached) = self.highlighter.as_mut() {
            cached.invalidate_from(current_line_number)
        }
    }

    pub fn delete_chars(&mut self, n: usize) {
        if self.content.len_chars() > 0 {
            let current_line_number = self.content.char_to_line(self.cursor);
            let end = (self.end_of_line(current_line_number) + 1).min(self.cursor + n);
            self.content.remove(self.cursor..end);
            self.move_cursor(self.cursor.min(self.end_of_line(current_line_number)));
            if let Some(cached) = self.highlighter.as_mut() {
                cached.invalidate_from(current_line_number)
            }
        }
    }

    pub fn back_delete_char(&mut self) {
        if self.cursor > 0 {
            self.move_cursor(self.cursor - 1);
            self.delete_chars(1);
        }
    }

    pub fn paste(&mut self, n: usize, text: &str) {
        for _ in 0..n {
            self.content.insert(self.cursor, text);
        }
        let line_number = self.content.char_to_line(self.cursor);
        if let Some(cached) = self.highlighter.as_mut() {
            cached.invalidate_from(line_number)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_of_line() {
        let config = Rc::new(Config::default());
        // empty line defaults to first char, even if there's none
        let buffer = Buffer::new(String::from(""), String::from(""), config.clone());
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(String::from("\n"), String::from(""), config.clone());
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(String::from("a\n"), String::from(""), config.clone());
        assert_eq!(buffer.end_of_line(0), 0);
        let buffer = Buffer::new(String::from("a\nbb\n"), String::from(""), config.clone());
        assert_eq!(buffer.end_of_line(1), 3);
        let buffer = Buffer::new(String::from("a\nbb"), String::from(""), config.clone());
        assert_eq!(buffer.end_of_line(1), 3);
        // out of bound returns last pos
        let buffer = Buffer::new(String::from("a\nbb\n"), String::from(""), config.clone());
        assert_eq!(buffer.end_of_line(2), 3);
        let buffer = Buffer::new(String::from("a\nbb\n"), String::from(""), config);
        assert_eq!(buffer.end_of_line(3), 3);
    }

    #[test]
    fn get_line() {
        let config = Rc::new(Config::default());

        let buffer = Buffer::new(String::from(""), String::from(""), config.clone());
        assert_eq!(buffer.get_line(0).map(String::from), None);

        let buffer = Buffer::new(String::from("\n"), String::from(""), config.clone());
        assert_eq!(
            buffer.get_line(0).map(String::from),
            Some(String::from("\n"))
        );
        assert_eq!(buffer.get_line(1).map(String::from), None);

        let buffer = Buffer::new(String::from("a\n\n"), String::from(""), config);
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
        let config = Rc::new(Config::default());
        let mut buffer = Buffer::new(String::from(""), String::from(""), config);
        buffer.delete_lines(1000);
        assert_eq!(buffer.get_line(0), None);
    }

    #[test]
    fn delete_char_out_of_bounds() {
        let config = Rc::new(Config::default());
        let mut buffer = Buffer::new(String::from(""), String::from(""), config);
        buffer.delete_chars(1000);
    }
}
