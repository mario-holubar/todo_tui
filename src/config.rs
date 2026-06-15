use serde::Deserialize;

const DEFAULT_CONFIG: &str = include_str!("../default_config.toml");

#[derive(Default, Debug, Deserialize)] // My new motto
pub struct Config {
    pub todo_file: String,
    pub file_indent: usize,
    pub render_indent: usize,
}

impl Config {
    pub fn load() -> Config {
        // TODO Load config file at runtime
        toml::from_str(DEFAULT_CONFIG).unwrap()
    }
}
