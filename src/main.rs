mod ted;

extern crate termion;

use self::ted::Ted;
use std::io;
use std::panic;
use std::env;
use termion::event::Event;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::{AlternateScreen, ToMainScreen};
use tui::backend::TermionBackend;
use tui::Terminal;

fn run() -> Result<(), io::Error> {
    let stdin = io::stdin();
    let stdout = io::stdout().into_raw_mode()?;
    let screen = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(screen);
    let terminal = Terminal::new(backend)?;

    let size = terminal.size()?;

    let mut ted = Ted::new(terminal, size.clone());
    ted.init()?;

    for argument in env::args() {
        println!("{}", argument);
        ted.file_open(argument);
    }
    ted.draw()?;

    for c in stdin.events() {
        match c.unwrap() {
            Event::Key(k) => {
                if ted.handle_key(k) {
                    break;
                }
            }
            _ => {}
        }
        ted.draw()?;
    }

    print!("{}", ToMainScreen);
    Ok(())
}

fn log<F: Fn() -> Result<(), io::Error>>(f: F) {
    if let Err(err) = f() {
        println!("{}", ToMainScreen);
        println!("Caught exception: {}", err);
    }
}

fn main() {
    if let Err(err) = panic::catch_unwind(|| log(run)) {
        print!("{}", ToMainScreen);
        panic::resume_unwind(err);
    }
}
