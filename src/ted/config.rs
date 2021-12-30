use crate::ted::BufReader;
use crate::ted::Cursor;
use crate::ted::SyntaxSet;
use crate::ted::ThemeSet;

pub struct Config {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub show_whitespace: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut theme_set = ThemeSet::load_defaults();
        if let Ok(theme) = ThemeSet::load_from_reader(&mut BufReader::new(Cursor::new(
            include_str!("../../assets/themes/ted.tmTheme").as_bytes(),
        ))) {
            theme_set.themes.insert("ted".to_string(), theme);
        }
        Self {
            theme_set,
            syntax_set: SyntaxSet::load_defaults_newlines(),
            show_whitespace: cfg!(debug_assertions),
        }
    }
}
