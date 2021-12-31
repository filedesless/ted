use crate::ted::buffer_widget::BufferWidget;
use buffer::{Buffer, InputMode};
use buffers::Buffers;
use command::Commands;
use config::Config;
use crossterm::cursor::{CursorShape, SetCursorShape};
use crossterm::event::KeyCode;
use crossterm::event::{KeyEvent, KeyModifiers};
use crossterm::execute;
use serde_json::json;
use serde_json::value::Value;
use std::io;
use std::io::BufReader;
use std::io::Cursor;
use std::rc::Rc;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use tui::backend::CrosstermBackend;
use tui::layout::Rect;
use tui::widgets::Paragraph;
use tui::Terminal;

mod buffer;
mod buffer_widget;
mod buffers;
mod cached_highlighter;
mod command;
mod config;

type TTerm = Terminal<CrosstermBackend<io::Stdout>>;

type TRes = Result<(), io::Error>;

fn format_space_chain(space_chain: &str) -> String {
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
    exit: bool,
    prompt: String,
    answer: String,
    message: String,
    space_chain: String,
    commands: Commands,
    prompt_callback: Option<fn(&mut Ted, String)>,
    universal_argument: Option<usize>,
    clipboard: String,
    config: Rc<Config>,
}

impl Ted {
    pub fn new(term: TTerm) -> Ted {
        let config = Rc::new(Config::default());
        Ted {
            term,
            buffers: Buffers::home(config.clone()),
            exit: false,
            prompt: String::default(),
            answer: String::default(),
            message: String::default(),
            space_chain: String::default(),
            commands: Commands::default(),
            prompt_callback: None,
            universal_argument: None,
            clipboard: String::default(),
            config,
        }
    }

    /// Redraw the buffer when we process an event
    pub fn draw(&mut self) -> TRes {
        let size = self.term.size()?;
        let buffer = self.buffers.focused_mut();
        let (_, line_number, column_number) = buffer.get_cursor();
        let status_line_number = size.height.saturating_sub(2) as usize;
        buffer.resize_window(status_line_number);
        let window = buffer.get_window();
        let (echo_line, cursor_x, cursor_y) = if self.prompt.is_empty() {
            (
                self.message.clone(),
                column_number as u16,
                (line_number - window.start) as u16,
            )
        } else {
            let line = format!("{}: {}", self.prompt, self.answer);
            let n = line.len();
            (line, n as u16, size.height.saturating_sub(1))
        };

        self.term.draw(|f| {
            let widget = BufferWidget {};
            let mut area = f.size();
            area.height -= 1;
            f.render_stateful_widget(widget, area, buffer);
            let echo = Paragraph::new(echo_line);
            f.render_widget(echo, Rect::new(0, area.height, area.width, 1));
            f.set_cursor(cursor_x, cursor_y);
        })?;

        Ok(())
    }

    fn new_buffer(&mut self, content: String) {
        let name = format!("Buffer #{}", self.buffers.len() + 1);
        self.message = format!("Created new buffer <{}>", name);
        self.buffers
            .new_buffer(Buffer::new(content, name, self.config.clone()));
    }

    fn run_command(&mut self, command: String) {
        let err = format!("Unrecognized command: {}", command);
        if let Some(command) = self.commands.get_by_name(&command) {
            command.get_action()(self);
        } else {
            self.message = err;
        }
    }

    pub fn file_open(&mut self, filepath: String) {
        let buffer = Buffer::from_file(&filepath, self.config.clone());
        self.message = match buffer {
            Ok(buffer) => {
                let message = format!("Created new buffer <{}>", buffer.name);
                self.buffers.new_buffer(buffer);
                message
            }
            Err(err) => format!("file_open({}): {}", filepath, err.to_string()),
        };
    }

    fn file_save(&mut self) {
        self.message = match self.buffers.focused_mut().overwrite_backend_file() {
            Ok(_) => String::from("File saved"),
            Err(e) => e.to_string(),
        };
    }

    fn next_buffer(&mut self) {
        if self.buffers.len() > 1 {
            self.buffers.cycle_next();
            self.message = format!("Switched to <{}>", self.buffers.focused().name);
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
        execute!(io::stdout(), SetCursorShape(CursorShape::Line)).unwrap();
    }

    fn space_mode(&mut self) {
        self.space_chain = " ".to_string();
        self.message = "SPC-".to_string();
    }

    fn format_space_chain(&self, completed: bool) -> String {
        let mut s = format_space_chain(&self.space_chain);
        s.push_str(if completed { "" } else { "-" });
        s
    }

    fn print_space_chain(&mut self, completed: bool) {
        self.message = self.format_space_chain(completed);
    }

    // returns wether the user asked to exit
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.space_chain.is_empty() {
            match key.code {
                KeyCode::Esc => {
                    self.normal_mode();
                    self.space_chain.clear();
                }
                KeyCode::Char(c) => self.space_chain.push(c),
                KeyCode::Tab => self.space_chain.push('\t'),
                _ => {}
            }
            let commands = self.commands.get_by_chain(&self.space_chain);
            match commands.len() {
                0 => {
                    self.normal_mode();
                    self.message = format!("{:?} is undefined", self.format_space_chain(true));
                    self.space_chain.clear();
                }
                1 if commands[0].chain_is(&self.space_chain) => {
                    let f = commands[0].get_action();
                    self.print_space_chain(true);
                    f(self);
                    self.normal_mode();
                    self.space_chain.clear();
                }
                _ => self.print_space_chain(false),
            }
        } else if !self.prompt.is_empty() {
            match key.code {
                KeyCode::Enter => {
                    self.normal_mode();
                    self.prompt.clear();
                    if let Some(f) = self.prompt_callback {
                        self.prompt_callback = None;
                        f(self, self.answer.clone());
                    }
                    self.answer.clear();
                }
                KeyCode::Esc => {
                    self.normal_mode();
                    self.prompt_callback = None;
                    self.prompt.clear();
                    self.answer.clear();
                }
                KeyCode::Backspace => {
                    let _ = self.answer.pop();
                }
                KeyCode::Char(c) => self.answer.push(c),
                _ => {}
            };
        } else {
            match self.buffers.focused().mode {
                InputMode::Normal => {
                    match key.code {
                        KeyCode::Char(c) => self.normal_mode_handle_key(c),
                        KeyCode::Esc => {
                            self.universal_argument = None;
                            self.message = "ESC".to_string();
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

    fn help_lang(&mut self) {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let obj: Vec<Value> = syntax_set
            .syntaxes()
            .iter()
            .map(|syntax| {
                json!({
                    "name": syntax.name,
                    "ext": syntax.file_extensions,
                    "first_line": syntax.first_line_match,
                })
            })
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&obj) {
            self.new_buffer(json);
            self.buffers
                .focused_mut()
                .set_language(&String::from("JSON"));
        }
    }

    fn set_lang(&mut self, name: String) {
        if !self.buffers.focused_mut().set_language(&name) {
            self.message = format!("Could not load lang {}", name);
        }
    }

    fn help_theme(&mut self) {
        let obj: Vec<Value> = self
            .config
            .theme_set
            .themes
            .iter()
            .map(|(name, theme)| {
                json!({
                    "name": name,
                    "theme": {
                        "prettyName": theme.name
                    }
                })
            })
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&obj) {
            self.new_buffer(json);
            self.buffers
                .focused_mut()
                .set_language(&String::from("JSON"));
        }
    }

    fn set_theme(&mut self, name: String) {
        if !self.buffers.focused_mut().set_theme(&name) {
            self.message = format!("Could not load theme {}", name);
        }
    }

    fn normal_mode_handle_key(&mut self, c: char) {
        let uarg = self.universal_argument;
        self.universal_argument = None;
        let n = uarg.unwrap_or(1);
        match c {
            ' ' => self.space_mode(),
            'i' => self.insert_mode(),
            'I' => {
                self.insert_mode();
                self.buffers.focused_mut().move_cursor_bol();
            }
            'a' => {
                self.insert_mode();
                self.buffers.focused_mut().move_cursor_right(1);
            }
            'A' => {
                self.insert_mode();
                self.buffers.focused_mut().move_cursor_eol();
            }
            'o' => {
                self.insert_mode();
                self.buffers.focused_mut().append_newline();
            }
            'O' => {
                self.insert_mode();
                self.buffers.focused_mut().prepend_newline();
            }
            'h' => self.buffers.focused_mut().move_cursor_left(n),
            'H' => self.buffers.focused_mut().move_cursor_bol(),
            'k' => self.buffers.focused_mut().move_cursor_up(n),
            'K' => self.buffers.focused_mut().page_up(n),
            'j' => self.buffers.focused_mut().move_cursor_down(n),
            'J' => self.buffers.focused_mut().page_down(n),
            'l' => self.buffers.focused_mut().move_cursor_right(n),
            'L' => self.buffers.focused_mut().move_cursor_eol(),
            's' => self.buffers.focused_mut().mark_selection(),
            'D' => self.buffers.focused_mut().delete_lines(n),
            'd' => self.buffers.focused_mut().delete_chars(n),
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
                    self.message = format!("C-u: {}", x);
                }
            }
            _ => {}
        }
    }
}
