use std::error::Error;

use keybinds::Keybinds;
use serde::Deserialize;

use crate::tui::Action;

const DEFAULT_CONFIG: &str = include_str!("../default_config.toml");

#[derive(Debug, Deserialize)] // My new motto
pub struct Config {
    pub general: GeneralConfig,
    pub normal_keymap: Keybinds<Action>,
    pub text_keymap: Keybinds<Action>,
    pub date_picker_keymap: Keybinds<Action>,
}

#[derive(Debug, Deserialize)]
pub struct GeneralConfig {
    pub todo_file: String,
    pub file_indent: usize,
    pub display_indent: usize,
}

impl Config {
    pub fn load() -> Result<Config, Box<dyn Error>> {
        let config: Config = toml::from_str(DEFAULT_CONFIG)?;
        Ok(config)
    }
}
