use buffer::{Buffer, Mode};
use command::Commands;
use ring_buffer::RingBuffer;
use std::io;
use termion::event::Key;
use termion::raw::RawTerminal;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::layout::Rect;
use tui::Terminal;

mod buffer;
mod command;
mod ring_buffer;

type TTerm = Terminal<TermionBackend<AlternateScreen<RawTerminal<io::Stdout>>>>;
// type TTerm = Terminal<TermionBackend<RawTerminal<io::Stdout>>>;

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
    buffers: RingBuffer,
    minibuffer: Buffer,
    exit: bool,
    prompt: String,
    termsize: Rect,
    space_chain: String,
    commands: Commands,
    prompt_callback: Option<fn(&mut Ted, String)>,
    universal_argument: Option<usize>,
    clipboard: String,
}

impl Ted {
    pub fn new(terminal: TTerm, termsize: Rect) -> Ted {
        Ted {
            term: terminal,
            buffers: RingBuffer::default(),
            minibuffer: Buffer::empty(),
            exit: false,
            prompt: String::default(),
            termsize,
            space_chain: String::default(),
            commands: Commands::default(),
            prompt_callback: None,
            universal_argument: None,
            clipboard: String::default(),
        }
    }

    // Initialize the terminal for use in Ted
    pub fn init(&mut self) -> TRes {
        print!("{}", termion::cursor::Save);
        self.normal_mode();
        self.term.clear()?;
        self.draw()?;
        self.term.show_cursor()
    }

    // Redraw the buffer when we process an event
    pub fn draw(&mut self) -> TRes {
        self.term.autoresize()?;
        self.termsize = self.term.size()?;
        self.term.hide_cursor()?;

        let buffer = &mut self.buffers.focused();

        // Draw the lines from the buffer
        self.term.set_cursor(0, 0)?;
        for (i, line) in buffer.get_lines().iter().enumerate() {
            self.term.set_cursor(0, i as u16)?;
            let eol = line.len();
            if line.len() < self.termsize.width as usize {
                let fill = " ".repeat(self.termsize.width as usize - line.len());
                println!("{}{}", line[0..eol].to_string(), fill);
            } else {
                println!("{}", line[0..eol].to_string());
            }
        }

        // Update tracked changes
        for change in buffer.get_changes() {
            match change {
                buffer::Change::ModifiedLine(linum) => {
                    let line = buffer.get_line(*linum);
                    if line.len() < self.termsize.width as usize {
                        let fill = " ".repeat(self.termsize.width as usize - line.len());
                        self.term.set_cursor(self.termsize.x, *linum as u16)?;
                        print!("{}{}", line, fill);
                    } else {
                        let eol = line.len().min(self.termsize.width as usize);
                        println!("{}", line[0..eol].to_string());
                    }
                }
                buffer::Change::DeletedLine(linum) => {
                    self.term.set_cursor(self.termsize.x, *linum as u16)?;
                    print!("{}", " ".repeat(self.termsize.width as usize));
                }
            }
        }
        buffer.clear_changes();

        // Prints out the status message
        let bottom = self.termsize.bottom();
        if bottom > 1 {
            self.term.set_cursor(0, bottom - 2)?;
            let status = match buffer.mode {
                Mode::Normal => "NORMAL MODE",
                Mode::Insert => "INSERT MODE",
            };
            let (linum, col) = buffer.get_cursor();
            let line = format!("{} ({}) {}:{}", buffer.name, status, linum + 1, col + 1);
            let fill = " ".repeat(self.termsize.width as usize - line.len());
            print!("{}{}", line, fill);
        }

        // Prints out the echo area
        self.term.set_cursor(0, bottom - 1)?;
        let line = self.minibuffer.get_current_line();
        if !self.prompt.is_empty() {
            let message = format!("{}: {}", self.prompt, line);
            let fill = " ".repeat(self.termsize.width as usize - message.len());
            print!("{}{}", message, fill);
            let (linum, col) = self.minibuffer.get_cursor();
            self.term.set_cursor(
                (col + self.prompt.len() + 2) as u16,
                self.termsize.bottom() + linum as u16,
            )?;
        } else {
            let fill = " ".repeat(self.termsize.width as usize - line.len());
            print!("{}{}", line, fill);
            let (linum, col) = buffer.get_cursor();
            let x = self.termsize.x + self.termsize.width.min(col as u16);
            self.term.set_cursor(x, linum as u16)?;
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

    fn next_buffer(&mut self) {
        let _ = self.term.clear();
        self.buffers.cycle_next();
        let message = format!("Switched to <{}>", self.buffers.focused().name);
        self.minibuffer.set_current_line(message.to_string());
    }

    fn insert_mode(&mut self) {
        self.buffers.focused().insert_mode();
        print!("{}", termion::cursor::SteadyBar);
    }

    fn normal_mode(&mut self) {
        self.buffers.focused().normal_mode();
        print!("{}", termion::cursor::SteadyBlock);
    }

    fn prompt_mode(&mut self, prompt: String, f: fn(&mut Ted, String)) {
        self.prompt = prompt;
        self.prompt_callback = Some(f);
        self.minibuffer.mode = Mode::Insert;
        self.minibuffer.set_current_line(String::default());
        print!("{}", termion::cursor::SteadyBar);
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
    pub fn handle_key(&mut self, key: Key) -> bool {
        if !self.space_chain.is_empty() {
            match key {
                Key::Esc => {
                    self.normal_mode();
                    self.space_chain = String::default();
                }
                Key::Char(c) => {
                    self.space_chain.push(c);
                }
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
            match key {
                Key::Char('\n') => {
                    let line = self.minibuffer.get_current_line().to_string();
                    self.normal_mode();
                    self.prompt = String::default();
                    if let Some(f) = self.prompt_callback {
                        self.prompt_callback = None;
                        f(self, line);
                    }
                }
                Key::Esc => {
                    self.normal_mode();
                    self.prompt = String::default();
                    self.prompt_callback = None;
                    self.minibuffer.set_current_line(String::default());
                }
                Key::Backspace => self.minibuffer.back_del_char(),
                Key::Char(c) => self.minibuffer.insert_char(c),
                _ => {}
            };
        } else {
            match self.buffers.focused().mode {
                Mode::Normal => {
                    match key {
                        Key::Char(c) => self.normal_mode_handle_key(c),
                        Key::Esc => {
                            self.universal_argument = None;
                            self.minibuffer.set_current_line("ESC".to_string());
                        }
                        _ => {}
                    };
                }
                Mode::Insert => {
                    match key {
                        Key::Backspace => self.buffers.focused().back_del_char(),
                        Key::Ctrl('c') => self.normal_mode(),
                        Key::Esc => self.normal_mode(),
                        Key::Char(c) => self.buffers.focused().insert_char(c),
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
            'i' => self.insert_mode(),
            'I' => {
                self.buffers.focused().move_cursor_bol();
                self.insert_mode();
            }
            'h' => self.buffers.focused().move_cursor_left(n),
            'l' => self.buffers.focused().move_cursor_right(n),
            'k' => self.buffers.focused().move_cursor_up(n),
            'j' => self.buffers.focused().move_cursor_down(n),
            'a' => self.append(),
            'A' => self.append_to_line(),
            'H' => self.buffers.focused().move_cursor_bol(),
            'L' => self.buffers.focused().move_cursor_eol(),
            'd' => {
                self.clipboard = String::default();
                for line in self.buffers.focused().del_lines(n) {
                    self.clipboard = format!("{}{}\n", self.clipboard, line);
                }
                // self.minibuffer.set_current_line(self.clipboard.to_string());
            }
            'x' => self.clipboard = self.buffers.focused().del_chars(n),
            'p' => self.paste(n),
            ' ' => self.space_mode(),
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

    fn append(&mut self) {
        self.insert_mode();
        self.buffers.focused().move_cursor_right(1);
    }

    fn append_to_line(&mut self) {
        self.insert_mode();
        self.buffers.focused().move_cursor_eol();
    }

    fn paste(&mut self, n: usize) {
        self.buffers.focused().insert_mode();
        for _ in 0..n {
            for c in self.clipboard.chars() {
                self.buffers.focused().insert_char(c);
            }
        }
        self.buffers.focused().normal_mode();
    }
}
