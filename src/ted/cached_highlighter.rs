use crate::ted::Config;
use ropey::Rope;
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;
use syntect::{highlighting::*, parsing::*};

#[cfg(debug_assertions)]
const STEP: usize = 100;
#[cfg(not(debug_assertions))]
const STEP: usize = 1000;

type State = (ParseState, HighlightState);

type Line = (String, Vec<(Style, Range<usize>)>);

pub struct CachedHighlighter {
    pub syntax: SyntaxReference,
    pub theme: Theme,
    highlighted_lines: Vec<Line>,
    config: Rc<Config>,
    /// (line_number => states) before parsing the line
    cache: BTreeMap<usize, State>,
}

impl CachedHighlighter {
    pub fn new(syntax: SyntaxReference, theme: Theme, config: Rc<Config>) -> Self {
        CachedHighlighter {
            syntax,
            config,
            theme,
            highlighted_lines: Vec::default(),
            cache: BTreeMap::default(),
        }
    }

    /// returns (line_number, state)
    fn latest_state(&mut self) -> (usize, State) {
        if let Some(&k) = self.cache.keys().max() {
            if let Some(state) = self.cache.get_mut(&k) {
                return (k, state.clone());
            }
        }
        let highlighter = Highlighter::new(&self.theme);
        let parse_state = ParseState::new(&self.syntax);
        let highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        let state = (parse_state, highlight_state);
        self.cache.insert(0, state.clone());
        (0, state)
    }

    /// must be called when content changes
    pub fn invalidate_from(&mut self, line_number: usize) {
        self.highlighted_lines.truncate(line_number);
        self.cache.retain(|k, _| k < &line_number);
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.invalidate_from(0);
    }

    /// returns up to range.len() lines
    pub fn get_highlighted_lines(&mut self, content: Rope, range: Range<usize>) -> Vec<Line> {
        if let Some(highlighted_lines) = self.highlighted_lines.get(range.clone()) {
            highlighted_lines.to_vec()
        } else {
            // get latest good state from cache
            let (line_number, (mut parse_state, mut highlight_state)) = self.latest_state();
            self.highlighted_lines.truncate(line_number);
            let highlighter = Highlighter::new(&self.theme);

            // work on content
            let lines = content
                .lines()
                .enumerate()
                .skip(line_number)
                .take(range.end.saturating_sub(line_number))
                .filter(|(_, s)| s.len_chars() != 0);
            for (i, line) in lines {
                if i % STEP == 0 {
                    let state = (parse_state.clone(), highlight_state.clone());
                    self.cache.insert(i, state);
                }
                let s = String::from(line);
                let changes = parse_state.parse_line(&s, &self.config.syntax_set);
                let ranges: Vec<(Style, Range<usize>)> =
                    RangedHighlightIterator::new(&mut highlight_state, &changes, &s, &highlighter)
                        .map(|(style, _, r)| (style, r))
                        .collect();
                self.highlighted_lines.push((s, ranges))
            }
            self.highlighted_lines[range.start..].to_vec()
        }
    }
}
