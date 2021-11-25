#![feature(backtrace)]
mod ted;

use self::ted::Ted;
use crossterm::event::{read, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use std::backtrace::Backtrace;
use std::{env, io, panic};
use tui::backend::CrosstermBackend;
use tui::Terminal;

fn run() -> Result<(), io::Error> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    enable_raw_mode().expect("Failed to enable raw mode");
    execute!(io::stdout(), EnterAlternateScreen)?;

    let size = terminal.size()?;

    let mut ted = Ted::new(terminal, size.clone());
    ted.init()?;

    for argument in env::args().skip(1) {
        println!("{}", argument);
        ted.file_open(argument);
    }
    ted.draw()?;

    // TODO: loop with event polling
    loop {
        match read()? {
            Event::Key(k) => {
                if ted.handle_key(k) {
                    break;
                }
            }
            // TODO: handle window resizing
            Event::Resize(_, _) => {}
            _ => {}
        }
        ted.draw()?;
    }

    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn main() -> Result<(), io::Error> {
    panic::set_hook(Box::new(|panic_info| {
        let backtrace = Backtrace::capture();
        disable_raw_mode().unwrap();
        execute!(io::stdout(), LeaveAlternateScreen).unwrap();
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            println!("panic occurred: {}", s);
        } else {
            println!("panic occurred");
        }
        println!("stack backtrace: {}", backtrace);
    }));

    run().or_else(|err| {
        println!("main returned an error: {}", err);
        Err(err)
    })
}
