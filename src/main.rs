mod config;
mod document;
mod tasks;
mod tui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let file = args.next();
    if matches!(file.as_deref(), Some("-h" | "--help")) {
        println!("Usage: todo_tui [FILE.md]");
        return Ok(());
    }
    if args.next().is_some() {
        return Err("Usage: todo_tui [FILE.md]".into());
    }
    let mut config = config::Config::load()?;
    if let Some(file) = file {
        config.general.todo_file = file;
    }
    let mut tui = tui::Tui::new(config);
    tui.main()?;
    Ok(())
}
