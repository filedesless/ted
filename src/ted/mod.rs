use buffer::{Buffer, Mode};
use command::Commands;
use nonempty::NonEmpty;
use std::io;
use termion::event::Key;
use termion::raw::RawTerminal;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::layout::Rect;
use tui::Terminal;

mod buffer;
mod command;

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
    buffers: NonEmpty<Buffer>,
    minibuffer: Buffer,
    exit: bool,
    prompt: String,
    termsize: Rect,
    space_chain: String,
    commands: Commands,
    prompt_callback: Option<fn(&mut Ted, String)>,
}

impl Ted {
    pub fn new(terminal: TTerm, termsize: Rect) -> Ted {
        Ted {
            term: terminal,
            buffers: NonEmpty::singleton(Buffer::default()),
            minibuffer: Buffer::empty(),
            exit: false,
            prompt: String::default(),
            termsize,
            space_chain: String::default(),
            commands: Commands::default(),
            prompt_callback: None,
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

        let buffer = &mut self.buffers.head;

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
        self.buffers.insert(0, Buffer::new(content, name));
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
                self.buffers.insert(0, buffer);
                message
            },
            Err(err) => format!("file_open({}): {}", filepath, err.to_string()),
        };
        self.minibuffer.set_current_line(message);
    }

    fn file_save(&mut self) {
        let msg = match self.buffers.head.overwrite_backend_file() {
            Ok(_) => String::from("File saved"),
            Err(e) => e.to_string(),
        };
        self.minibuffer.set_current_line(msg);
    }

    // TODO: clone aaaaaaaaaaaaaaaaa
    fn next_buffer(&mut self) {
        let _ = self.term.clear();
        let (head, tail) = self.buffers.split_first();
        let mut v = tail.to_vec();
        v.push(head.clone());
        if let Some(n) = NonEmpty::from_vec(v) {
            self.buffers = n;
        }
        let message = format!("Switched to <{}>", self.buffers.head.name);
        self.minibuffer.set_current_line(message.to_string());
    }

    fn insert_mode(&mut self) {
        self.buffers.head.insert_mode();
        print!("{}", termion::cursor::SteadyBar);
    }

    fn normal_mode(&mut self) {
        self.buffers.head.normal_mode();
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
            match self.buffers.head.mode {
                Mode::Normal => {
                    match key {
                        Key::Char('i') => self.insert_mode(),
                        Key::Char('I') => {
                            self.buffers.head.move_cursor_bol();
                            self.insert_mode();
                        }
                        Key::Char('h') => self.buffers.head.move_cursor_left(),
                        Key::Char('l') => self.buffers.head.move_cursor_right(),
                        Key::Char('k') => self.buffers.head.move_cursor_up(),
                        Key::Char('j') => self.buffers.head.move_cursor_down(),
                        Key::Char('a') => self.append(),
                        Key::Char('A') => self.append_to_line(),
                        Key::Char('H') => self.buffers.head.move_cursor_bol(),
                        Key::Char('L') => self.buffers.head.move_cursor_eol(),
                        Key::Char('d') => self.buffers.head.del_line(),
                        Key::Char('x') => self.buffers.head.del_char(),
                        Key::Char(' ') => self.space_mode(),
                        _ => {}
                    };
                }
                Mode::Insert => {
                    // self.minibuffer.set_current_line(format!("{:?}", key));
                    match key {
                        Key::Char('\n') => self.buffers.head.new_line(),
                        Key::Backspace => self.buffers.head.back_del_char(),
                        Key::Ctrl('c') => self.normal_mode(),
                        Key::Esc => self.normal_mode(),
                        Key::Char(c) => self.buffers.head.insert_char(c),
                        _ => {}
                    };
                }
            };
        }
        self.exit
    }

    fn append(&mut self) {
        self.insert_mode();
        self.buffers.head.move_cursor_right();
    }

    fn append_to_line(&mut self) {
        self.buffers.head.move_cursor_eol();
        self.append();
    }
}
