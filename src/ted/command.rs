use crate::Ted;

pub struct Command {
    pub name: String,
    pub desc: String,
    pub chain: Option<String>,
    action: fn(&mut Ted) -> (),
}

impl Command {
    pub fn get_action(&self) -> fn(&mut Ted) {
        self.action
    }

    pub fn chain_is(&self, other: &String) -> bool {
        self.chain
            .as_ref()
            .map(|chain| chain == other)
            .unwrap_or(false)
    }
}

pub struct Commands {
    pub commands: Vec<Command>,
}

impl Default for Commands {
    fn default() -> Self {
        Commands {
            commands: vec![
                Command {
                    name: "space".to_string(),
                    desc: "Enters command mode".to_string(),
                    chain: Some("  ".to_string()),
                    action: (|t| t.prompt_mode("Command".to_string(), Ted::run_command)),
                },
                Command {
                    name: "quit".to_string(),
                    desc: "Exits Ted".to_string(),
                    chain: Some(" q".to_string()),
                    action: (|t| t.exit = true),
                },
                Command {
                    name: "new_empty_buffer".to_string(),
                    desc: "Creates a new empty buffer".to_string(),
                    chain: Some(" fn".to_string()),
                    action: (|t| t.new_buffer(String::default())),
                },
                Command {
                    name: "file_open".to_string(),
                    desc: "Opens given file".to_string(),
                    chain: Some(" fo".to_string()),
                    action: (|t| t.prompt_mode("File open".to_string(), Ted::file_open)),
                },
                Command {
                    name: "file_save".to_string(),
                    desc: "Saves the buffer to a file".to_string(),
                    chain: Some(" fs".to_string()),
                    action: Ted::file_save,
                },
                Command {
                    name: "next_buffer".to_string(),
                    desc: "Opens the next buffer".to_string(),
                    chain: Some(" \t".to_string()),
                    action: Ted::next_buffer,
                },
            ],
        }
    }
}

impl Commands {
    pub fn get_by_chain(&self, prefix: &String) -> Vec<&Command> {
        self.commands
            .iter()
            .filter(|command| {
                if let Some(chain) = &command.chain {
                    return chain.starts_with(prefix);
                } else {
                    return false;
                }
            })
            .collect()
    }

    pub fn get_by_name(&self, needle: &String) -> Option<&Command> {
        self.commands.iter().find(|command| &command.name == needle)
    }
}

#[cfg(test)]
mod tests {
    use crate::ted::Commands;
    use std::collections::HashSet;
    use std::iter::FromIterator;

    #[test]
    fn no_dup_command_chain() {
        let commands = Commands::default();
        let v: Vec<String> = commands
            .commands
            .iter()
            .filter_map(|c| c.chain.as_ref().map(|chain| chain.to_string()))
            .collect();
        let n = v.len();
        let h: HashSet<String> = HashSet::from_iter(v);
        assert_eq!(n, h.len());
    }

    #[test]
    fn get_by_chain() {
        let commands = Commands::default();
        let full_list = commands.get_by_chain(&" ".to_string());
        assert!(full_list.len() > 1);
        let exact_match = commands.get_by_chain(&"  ".to_string());
        assert!(exact_match.len() == 1);
        let empty_list = commands.get_by_chain(&"   ".to_string());
        assert!(empty_list.len() == 0);
    }
}
