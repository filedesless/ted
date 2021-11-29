mod ted;

use self::ted::Ted;
use crossterm::event::{read, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use std::{env, io, panic};
use tui::backend::CrosstermBackend;
use tui::Terminal;

fn run() -> Result<(), io::Error> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    enable_raw_mode().expect("Failed to enable raw mode");
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;

    let size = terminal.size()?;

    let mut ted = Ted::new(terminal, size);

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
            Event::Resize(_, _) => ted.handle_resize()?,
            _ => {}
        }
        ted.draw()?;
    }

    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(io::stdout(), LeaveAlternateScreen)
}

fn main() -> Result<(), io::Error> {
    let default_panic = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        disable_raw_mode().unwrap();
        execute!(io::stdout(), LeaveAlternateScreen).unwrap();
        default_panic(panic_info);
    }));

    run().map_err(|err| {
        disable_raw_mode().unwrap();
        execute!(io::stdout(), LeaveAlternateScreen).unwrap();
        println!("main returned an error: {:?}", err);
        err
    })
}
