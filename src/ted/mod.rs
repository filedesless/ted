use buffer::{Buffer, EditMode, InputMode};
use buffers::Buffers;
use command::Commands;
use crossterm::cursor::{CursorShape, SetCursorShape};
use crossterm::event::KeyCode;
use crossterm::event::{KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::style::{Color, SetBackgroundColor};

use std::io;
use tui::backend::CrosstermBackend;
use tui::layout::Rect;
use tui::Terminal;

mod buffer;
mod buffers;
mod command;

type TTerm = Terminal<CrosstermBackend<io::Stdout>>;

type TRes = Result<(), io::Error>;

fn format_space_chain(space_chain: &String) -> String {
    let v: Vec<String> = space_chain
        .chars()
        .map(|c| match c {
            ' ' => String::from("SPC"),
            '\t' => String::from("TAB"),
            _ => String::from(c),
        })
        .collect();
    v.join(" ")
}

pub struct Ted {
    term: TTerm,
    buffers: Buffers,
    minibuffer: Buffer,
    exit: bool,
    prompt: String,
    termsize: Rect,
    space_chain: String,
    commands: Commands,
    prompt_callback: Option<fn(&mut Ted, String)>,
    universal_argument: Option<usize>,
    clipboard: String,
    status_line: String,
    echo_line: String,
}

impl Ted {
    pub fn new(terminal: TTerm, termsize: Rect) -> Ted {
        Ted {
            term: terminal,
            buffers: Buffers::default(),
            minibuffer: Buffer::empty(),
            exit: false,
            prompt: String::default(),
            termsize,
            space_chain: String::default(),
            commands: Commands::default(),
            prompt_callback: None,
            universal_argument: None,
            clipboard: String::default(),
            status_line: String::default(),
            echo_line: String::default(),
        }
    }

    pub fn handle_resize(&mut self) -> TRes {
        self.term.autoresize()?;
        self.termsize = self.term.size()?;
        self.buffers.focused_mut().dirty = true;
        Ok(())
    }

    /// Redraw the buffer when we process an event
    pub fn draw(&mut self) -> TRes {
        let width = self.termsize.width as usize;
        let height = self.termsize.height as usize;
        let buffer = self.buffers.focused_mut();
        let status_line_number = height.saturating_sub(2);
        let echo_line_number = height.saturating_sub(1);
        buffer.resize_window(status_line_number);
        let (cursor, line_number, column_number) = buffer.get_cursor();

        // Redraw buffer
        // TODO use tui::buffer::Buffer instead of printing to stdout
        if buffer.dirty {
            self.term.hide_cursor()?;
            let mut current_line = 0;
            let mut line_length = 0;
            self.term.set_cursor(0, current_line as u16)?;
            let selection = buffer.get_selection_range();
            if let Some((chars, first_position)) = buffer.get_chars() {
                for (offset, character) in chars.enumerate() {
                    let char_index = first_position + offset;
                    line_length += 1;
                    if let Some(true) = selection.as_ref().map(|s| char_index == s.start) {
                        execute!(io::stdout(), SetBackgroundColor(Color::DarkGrey))?;
                    }
                    if character == '\n' {
                        current_line += 1;
                        print!("{}", " ".repeat(width.saturating_sub(line_length)));
                        self.term.set_cursor(0, current_line as u16)?;
                        line_length = 0;
                    } else if line_length >= width {
                        continue;
                    } else {
                        print!("{}", character);
                    }
                    if let Some(true) = selection.as_ref().map(|s| char_index == s.end) {
                        execute!(io::stdout(), SetBackgroundColor(Color::Black))?;
                    }
                    if current_line >= status_line_number {
                        break;
                    }
                }
            }

            for i in current_line..status_line_number {
                self.term.set_cursor(0, i as u16)?;
                println!("{}", " ".repeat(width));
            }

            buffer.dirty = false;
        }

        // Prints out the status message
        let status = match (buffer.mode, buffer.edit_mode) {
            (InputMode::Normal, EditMode::Char) => "NORMAL CHAR MODE",
            (InputMode::Normal, EditMode::Line) => "NORMAL LINE MODE",
            (InputMode::Insert, EditMode::Char) => "INSERT CHAR MODE",
            (InputMode::Insert, EditMode::Line) => "INSERT LINE MODE",
        };
        let window = buffer.get_window();
        let line = format!(
            "{} - {} - ({}x{}) at {} ({}:{}), from {} to {}",
            buffer.name,
            status,
            width,
            height,
            cursor,
            line_number,
            column_number,
            window.start,
            window.end,
        );
        if line != self.status_line {
            self.term.hide_cursor()?;
            self.term.set_cursor(0, status_line_number as u16)?;
            let fill = " ".repeat(self.termsize.width as usize - line.len());
            print!("{}{}", line, fill);
            self.status_line = line;
        }

        // Prints out the echo area
        let line = self.minibuffer.get_current_line().unwrap_or_default();
        if line != self.echo_line {
            self.term.hide_cursor()?;
            self.term.set_cursor(0, echo_line_number as u16)?;
            if !self.prompt.is_empty() {
                let message = format!("{}: {}", self.prompt, line);
                let fill = " ".repeat(self.termsize.width as usize - message.len());
                print!("{}{}", message, fill);
                let (_, linum, col) = self.minibuffer.get_cursor();
                self.term.set_cursor(
                    (col + self.prompt.len() + 2) as u16,
                    self.termsize.bottom() + linum as u16,
                )?;
            } else {
                let fill = " ".repeat(self.termsize.width as usize - line.len());
                print!("{}{}", line, fill);
            }
            self.echo_line = line;
        }

        self.term.set_cursor(
            column_number as u16,
            (line_number - window.start) as u16,
        )?;
        self.term.show_cursor()
    }

    fn new_buffer(&mut self, content: String) {
        let _ = self.term.clear();
        let name = format!("Buffer #{}", self.buffers.len() + 1);
        let message = format!("Created new buffer <{}>", name);
        self.buffers.new_buffer(Buffer::new(content, name));
        self.minibuffer.set_current_line(String::from(message));
    }

    fn run_command(&mut self, command: String) {
        let err = format!("Unrecognized command: {}", command);
        if let Some(command) = self.commands.get_by_name(&command) {
            command.get_action()(self);
        } else {
            self.minibuffer.set_current_line(err);
        }
    }

    pub fn file_open(&mut self, filepath: String) {
        let _ = self.term.clear();
        let message = match Buffer::from_file(&filepath) {
            Ok(buffer) => {
                let message = format!("Created new buffer <{}>", buffer.name);
                self.buffers.new_buffer(buffer);
                message
            }
            Err(err) => format!("file_open({}): {}", filepath, err.to_string()),
        };
        self.minibuffer.set_current_line(message);
    }

    fn file_save(&mut self) {
        let msg = match self.buffers.focused_mut().overwrite_backend_file() {
            Ok(_) => String::from("File saved"),
            Err(e) => e.to_string(),
        };
        self.minibuffer.set_current_line(msg);
    }

    fn next_buffer(&mut self) {
        if self.buffers.len() > 1 {
            let _ = self.term.clear();
            self.buffers.cycle_next();
            self.buffers.focused_mut().dirty = true;
            let message = format!("Switched to <{}>", self.buffers.focused().name);
            self.minibuffer.set_current_line(message.to_string());
        }
    }

    fn insert_mode(&mut self) {
        self.buffers.focused_mut().insert_mode();
        execute!(io::stdout(), SetCursorShape(CursorShape::Line)).unwrap();
    }

    fn normal_mode(&mut self) {
        self.buffers.focused_mut().normal_mode();
        execute!(io::stdout(), SetCursorShape(CursorShape::Block)).unwrap();
    }

    fn prompt_mode(&mut self, prompt: String, f: fn(&mut Ted, String)) {
        self.prompt = prompt;
        self.prompt_callback = Some(f);
        self.minibuffer.mode = InputMode::Insert;
        self.minibuffer.set_current_line(String::default());
        execute!(io::stdout(), SetCursorShape(CursorShape::Line)).unwrap();
    }

    fn space_mode(&mut self) {
        self.space_chain = String::from(" ");
        self.minibuffer.set_current_line("SPC-".to_string());
    }

    fn format_space_chain(&self, completed: bool) -> String {
        let mut s = format_space_chain(&self.space_chain);
        s.push_str(if completed { "" } else { "-" });
        s
    }

    fn print_space_chain(&mut self, completed: bool) {
        self.minibuffer
            .set_current_line(self.format_space_chain(completed));
    }

    // returns wether the user asked to exit
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.space_chain.is_empty() {
            match key.code {
                KeyCode::Esc => {
                    self.normal_mode();
                    self.space_chain = String::default();
                }
                KeyCode::Char(c) => self.space_chain.push(c),
                KeyCode::Tab => self.space_chain.push('\t'),
                _ => {}
            }
            let commands = self.commands.get_by_chain(&self.space_chain);
            match commands.len() {
                0 => {
                    self.normal_mode();
                    self.minibuffer.set_current_line(format!(
                        "{:?} is undefined",
                        self.format_space_chain(true)
                    ));
                    self.space_chain = String::default();
                }
                1 if commands[0].chain_is(&self.space_chain) => {
                    let f = commands[0].get_action();
                    self.print_space_chain(true);
                    f(self);
                    self.normal_mode();
                    self.space_chain = String::default();
                }
                _ => self.print_space_chain(false),
            }
        } else if !self.prompt.is_empty() {
            match key.code {
                KeyCode::Enter => {
                    let line = self.minibuffer.get_current_line().unwrap().to_string();
                    self.normal_mode();
                    self.prompt = String::default();
                    if let Some(f) = self.prompt_callback {
                        self.prompt_callback = None;
                        f(self, line);
                    }
                }
                KeyCode::Esc => {
                    self.normal_mode();
                    self.prompt = String::default();
                    self.prompt_callback = None;
                    self.minibuffer.set_current_line(String::default());
                }
                KeyCode::Backspace => self.minibuffer.back_delete_char(),
                KeyCode::Char(c) => self.minibuffer.insert_char(c),
                _ => {}
            };
        } else {
            match self.buffers.focused().mode {
                InputMode::Normal => {
                    match key.code {
                        KeyCode::Char(c) => self.normal_mode_handle_key(c),
                        KeyCode::Tab => self.buffers.focused_mut().cycle_submode(),
                        KeyCode::Esc => {
                            self.universal_argument = None;
                            self.minibuffer.set_current_line("ESC".to_string());
                            self.buffers.focused_mut().remove_selection();
                        }
                        _ => {}
                    };
                }
                InputMode::Insert => {
                    match key.code {
                        KeyCode::Backspace => self.buffers.focused_mut().back_delete_char(),
                        KeyCode::Enter => self.buffers.focused_mut().insert_char('\n'),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.normal_mode()
                        }
                        KeyCode::Esc => self.normal_mode(),
                        KeyCode::Char(c) => self.buffers.focused_mut().insert_char(c),
                        _ => {}
                    };
                }
            };
        }
        self.exit
    }

    fn normal_mode_handle_key(&mut self, c: char) {
        let uarg = self.universal_argument;
        self.universal_argument = None;
        let n = uarg.unwrap_or(1);
        match c {
            ' ' => self.space_mode(),
            'i' => self.insert_mode(),
            'h' => self.buffers.focused_mut().move_cursor_left(n),
            'H' => self.buffers.focused_mut().move_cursor_bol(),
            'k' => self.buffers.focused_mut().move_cursor_up(n),
            'K' => self.buffers.focused_mut().page_up(n),
            'j' => self.buffers.focused_mut().move_cursor_down(n),
            'J' => self.buffers.focused_mut().page_down(n),
            'l' => self.buffers.focused_mut().move_cursor_right(n),
            'L' => self.buffers.focused_mut().move_cursor_eol(),
            's' => self.buffers.focused_mut().mark_selection(),
            'd' => self.buffers.focused_mut().delete(n),
            'p' => self.buffers.focused_mut().paste(n, &self.clipboard),
            'c' => todo!(), // copy
            'u' => todo!(), // undo
            'r' => todo!(), // redo
            'f' => todo!(), // find
            'g' => todo!(), // goto
            c if c.is_digit(10) => {
                let current = uarg.unwrap_or(0);
                if let Some(u) = c.to_digit(10) {
                    let x = current * 10 + u as usize;
                    self.universal_argument = Some(x);
                    self.minibuffer.set_current_line(format!("C-u: {}", x));
                }
            }
            _ => {}
        }
    }
}
