use std::error::Error;

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
        let config: Config = toml::from_str(DEFAULT_CONFIG)?;
        Ok(config)
    }
}
