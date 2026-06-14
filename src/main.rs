mod config;
mod tasks;
mod tui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tui::main()?;
    Ok(())
}
