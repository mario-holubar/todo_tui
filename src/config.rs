use std::error::Error;

use keybinds::Keybinds;
use serde::Deserialize;

use crate::tui::Action;

const DEFAULT_CONFIG: &str = include_str!("../default_config.toml");

#[derive(Default, Debug, Deserialize)] // My new motto
pub struct Config {
    pub todo_file: String,
    pub file_indent: usize,
    pub render_indent: usize,
    pub normal_keymap: Keybinds<Action>,
    pub text_keymap: Keybinds<Action>,
}

impl Config {
    pub fn load() -> Result<Config, Box<dyn Error>> {
        let config: Config = toml::from_str(DEFAULT_CONFIG)?;
        Ok(config)
    }
}
