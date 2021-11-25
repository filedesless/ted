use buffer::Change::{DrawLine, DrawLinesFrom};
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

    // Initialize the terminal for use in Ted
    pub fn init(&mut self) -> TRes {
        self.normal_mode();
        self.term.clear()?;
        self.draw()?;
        self.term.hide_cursor()
    }

    // Redraw the buffer when we process an event
    pub fn draw(&mut self) -> TRes {
        self.term.autoresize()?;
        self.termsize = self.term.size()?;
        let buffer = self.buffers.focused();
        let bottom = self.termsize.bottom();
        let status_line_number = bottom.saturating_sub(2);
        let echo_line_number = bottom.saturating_sub(1);
        let width = self.termsize.width as usize;
        let draw_line = |linum| {
            if let Some(line) = buffer.get_line(linum) {
                let s = String::from(line);
                let trimmed_line = s.trim();
                if trimmed_line.len() < width {
                    let fill = " ".repeat(width - trimmed_line.len());
                    println!("{}{}", trimmed_line, fill);
                } else {
                    println!("{}", trimmed_line.get(..width).unwrap());
                }
            } else {
                println!("~{}", " ".repeat(width));
            }
        };

        // Update tracked changes
        let changes = buffer.get_changes();
        if !changes.is_empty() {
            self.term.hide_cursor()?;
        }
        for change in buffer.get_changes() {
            match change {
                DrawLine(linum) => {
                    self.term.set_cursor(0, *linum as u16)?;
                    draw_line(*linum);
                }
                DrawLinesFrom(start_line) => {
                    for line_number in *start_line..(status_line_number as usize) {
                        self.term.set_cursor(0, line_number as u16)?;
                        draw_line(line_number);
                    }
                }
            }
        }

        buffer.clear_changes();

        // Apply selection
        if let Some((x, y)) = buffer.get_selection_coord() {
            self.term.set_cursor(x as u16, y as u16)?;
            execute!(io::stdout(), SetBackgroundColor(Color::DarkGrey)).unwrap();
        }

        // Prints out the status message
        let status = match (buffer.mode, buffer.edit_mode) {
            (InputMode::Normal, EditMode::Char) => "NORMAL CHAR MODE",
            (InputMode::Normal, EditMode::Line) => "NORMAL LINE MODE",
            (InputMode::Insert, EditMode::Char) => "INSERT CHAR MODE",
            (InputMode::Insert, EditMode::Line) => "INSERT LINE MODE",
        };
        let (pos, linum, col) = buffer.get_cursor();
        let line = format!(
            "{} - {} - {} {}:{}",
            buffer.name,
            status,
            pos,
            linum + 1,
            col + 1
        );
        if line != self.status_line {
            self.term.hide_cursor()?;
            self.term.set_cursor(0, status_line_number)?;
            let fill = " ".repeat(self.termsize.width as usize - line.len());
            print!("{}{}", line, fill);
            self.status_line = line;
        }

        // Prints out the echo area
        let line = self.minibuffer.get_current_line().unwrap_or_default();
        if line != self.echo_line {
            self.term.hide_cursor()?;
            self.term.set_cursor(0, echo_line_number)?;
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
                let (_, linum, col) = buffer.get_cursor();
                let x = self.termsize.x + self.termsize.width.min(col as u16);
                self.term.set_cursor(x, linum as u16)?;
            }
            self.echo_line = line;
            self.minibuffer.clear_changes();
        }

        if (col as u16, linum as u16) != self.term.get_cursor()? {
            self.term.set_cursor(col as u16, linum as u16)?;
        }
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
        let msg = match self.buffers.focused().overwrite_backend_file() {
            Ok(_) => String::from("File saved"),
            Err(e) => e.to_string(),
        };
        self.minibuffer.set_current_line(msg);
    }

    // TODO: trigger a redraw
    fn next_buffer(&mut self) {
        let _ = self.term.clear();
        self.buffers.cycle_next();
        let message = format!("Switched to <{}>", self.buffers.focused().name);
        self.minibuffer.set_current_line(message.to_string());
    }

    fn insert_mode(&mut self) {
        self.buffers.focused().insert_mode();
        execute!(io::stdout(), SetCursorShape(CursorShape::Line)).unwrap();
    }

    fn normal_mode(&mut self) {
        self.buffers.focused().normal_mode();
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
                        KeyCode::Tab => self.buffers.focused().cycle_submode(),
                        KeyCode::Esc => {
                            self.universal_argument = None;
                            self.minibuffer.set_current_line("ESC".to_string());
                            self.buffers.focused().remove_selection();
                        }
                        _ => {}
                    };
                }
                InputMode::Insert => {
                    match key.code {
                        KeyCode::Backspace => self.buffers.focused().back_delete_char(),
                        KeyCode::Enter => self.buffers.focused().insert_char('\n'),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.normal_mode()
                        }
                        KeyCode::Esc => self.normal_mode(),
                        KeyCode::Char(c) => self.buffers.focused().insert_char(c),
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
            'h' => self.buffers.focused().move_cursor_left(n),
            'j' => self.buffers.focused().move_cursor_down(n),
            'k' => self.buffers.focused().move_cursor_up(n),
            'l' => self.buffers.focused().move_cursor_right(n),
            's' => self.buffers.focused().mark_selection(),
            'd' => self.buffers.focused().delete(n),
            'p' => self.buffers.focused().paste(n, &self.clipboard),
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
