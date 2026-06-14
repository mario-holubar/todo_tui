mod config;
mod tasks;
mod tui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tui = tui::Tui::new();
    tui.main()?;
    Ok(())
}
