use ropey::Rope;
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;
use syntect::util::as_24_bit_terminal_escaped;
use syntect::{highlighting::*, parsing::*};

#[cfg(debug_assertions)]
const STEP: usize = 100;
#[cfg(not(debug_assertions))]
const STEP: usize = 1000;

type State = (ParseState, HighlightState);

pub struct CachedHighlighter {
    /// [(escaped_line, original_len)]
    highlighted_lines: Vec<(String, usize)>,
    syntax: SyntaxReference,
    syntax_set: Rc<SyntaxSet>,
    theme: Theme,
    /// (line_number => states) before parsing the line
    cache: BTreeMap<usize, State>,
}

impl CachedHighlighter {
    pub fn new(syntax: SyntaxReference, syntax_set: Rc<SyntaxSet>, theme: Theme) -> Self {
        CachedHighlighter {
            syntax,
            syntax_set,
            theme,
            highlighted_lines: Vec::default(),
            cache: BTreeMap::default(),
        }
    }

    /// returns (line_number, state)
    fn latest_state(&mut self) -> (usize, State) {
        if let Some(k) = self.cache.keys().max() {
            let key = k.clone();
            if let Some(state) = self.cache.get_mut(&key) {
                return (key, state.clone());
            }
        }
        let mut highlighter = Highlighter::new(&self.theme);
        let parse_state = ParseState::new(&self.syntax);
        let highlight_state = HighlightState::new(&mut highlighter, ScopeStack::new());
        (0, (parse_state, highlight_state))
    }

    /// must be called when content changes
    pub fn invalidate_from(&mut self, line_number: usize) {
        self.highlighted_lines.truncate(line_number);
        self.cache.retain(|k, _| k < &line_number);
    }

    /// returns up to range.len() lines of [(escaped_line, original_len)]
    pub fn get_highlighted_lines(
        &mut self,
        content: Rope,
        range: Range<usize>,
    ) -> Vec<(String, usize)> {
        if let Some(highlighted_lines) = self.highlighted_lines.get(range.clone()) {
            highlighted_lines.to_vec()
        } else {
            // get latest good state from cache
            let (line_number, (mut parse_state, mut highlight_state)) = self.latest_state();
            let highlighter = Highlighter::new(&self.theme);

            // work on self.content
            let lines = content
                .lines()
                .enumerate()
                .skip(line_number)
                .take(range.end);
            for (i, line) in lines {
                if i % STEP == 0 {
                    let state = (parse_state.clone(), highlight_state.clone());
                    self.cache.insert(i, state);
                }
                let s = String::from(line);
                let changes = parse_state.parse_line(&s, &self.syntax_set);
                let ranges: Vec<(Style, &str)> =
                    HighlightIterator::new(&mut highlight_state, &changes, &s, &highlighter)
                        .collect();
                if i >= self.highlighted_lines.len() {
                    let highlighted_line = as_24_bit_terminal_escaped(&ranges[..], true);
                    let len = line.len_chars();
                    self.highlighted_lines.push((highlighted_line, len))
                }
            }
            let n = self.highlighted_lines.len();
            self.highlighted_lines
                .get(range.start..n)
                .unwrap_or(&Vec::default())
                .to_vec()
        }
    }
}
