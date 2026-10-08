use std::{error::Error, fs, path::PathBuf};

use keybinds::Keybinds;
use ratatui::style::Color;
use serde::Deserialize;

use crate::tui::Action;

const DEFAULT_CONFIG: &str = include_str!("../default_config.toml");

#[derive(Debug, Deserialize)] // My new motto
pub struct Config {
    pub general: GeneralConfig,
    pub colors: Colors,
    pub normal_keymap: Keybinds<Action>,
    pub text_keymap: Keybinds<Action>,
    pub date_picker_keymap: Keybinds<Action>,
}

#[derive(Debug, Deserialize)]
pub struct Colors {
    pub text: Color,
    pub background: Color,
    pub border: Color,
    pub completed: Color,
    pub overdue: Color,
    pub upcoming: Color,
    pub muted: Color,
    pub actionable: Color,
    pub selection_bg: Color,
    pub descendant_bg: Color,
    pub search_fg: Color,
    pub search_bg: Color,
    pub active_tab_fg: Color,
    pub active_tab_bg: Color,
    pub inactive_tab: Color,
    pub calendar_selection_bg: Color,
}

#[derive(Debug, Deserialize)]
pub struct GeneralConfig {
    pub todo_file: String,
    pub file_indent: usize,
    pub display_indent: usize,
    pub autosave: bool,
}

impl Config {
    pub fn load() -> Result<Config, Box<dyn Error>> {
        let mut config: toml::Value = toml::from_str(DEFAULT_CONFIG)?;
        if let Some(home) = std::env::var_os("HOME") {
            let path = PathBuf::from(home).join(".config/todo_tui/config.toml");
            match fs::read_to_string(path) {
                Ok(contents) => {
                    let overrides: toml::Value = toml::from_str(&contents)?;
                    merge(&mut config, overrides);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(config.try_into()?)
    }
}

fn merge(defaults: &mut toml::Value, overrides: toml::Value) {
    match (defaults, overrides) {
        (toml::Value::Table(defaults), toml::Value::Table(overrides)) => {
            for (key, value) in overrides {
                if let Some(default) = defaults.get_mut(&key) {
                    merge(default, value);
                } else {
                    defaults.insert(key, value);
                }
            }
        }
        (default, override_value) => *default = override_value,
    }
}
