use std::fs;

/// Represents a single todo item.
#[derive(Debug, Clone)]
struct Task {
    title: String,
    indent: usize,
    completed: bool,
}

/// Parse the markdown todo list content into a vector of [`Task`] structs.
///
/// Each line matching `- [ ]` (unchecked) or `- [x]`/`- [X]` (checked)
/// is converted into a `Task`. All other lines are skipped.
fn parse_tasks(content: &str) -> Vec<Task> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("- [") {
                return None;
            }

            // Find the closing bracket
            let close_bracket = trimmed.find(']')?;
            let checkbox_content = trimmed[3..close_bracket].trim();

            let completed = matches!(checkbox_content, "x" | "X");
            let title = trimmed[close_bracket + 1..].trim().to_string();

            if title.is_empty() {
                return None;
            }

            let indent = line.len() - trimmed.len();

            Some(Task { title, completed, indent })
        })
        .collect()
}

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    layout::Constraint,
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Check if the given key event should quit the application.
fn should_quit(key: &event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c' | 'd') if key.modifiers == KeyModifiers::CONTROL => true,
        _ => false,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Disable raw mode on panic
    std::panic::set_hook(Box::new(|info| {
        let mut stdout = std::io::stdout();
        stdout.execute(terminal::LeaveAlternateScreen).unwrap();
        terminal::disable_raw_mode().unwrap();
        eprintln!("{info}");
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Read the todo file
    let content = fs::read_to_string("todo.md")?;
    let tasks = parse_tasks(&content);

    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            // Split layout: header takes 2 rows, content takes the rest
            let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(2)]).split(area);

            // Render header block with quit instructions
            let header = Paragraph::new(Line::from(Span::styled(
                " Todos (q/ctrl-c/ctrl-d to quit) ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
            );
            frame.render_widget(header, chunks[0]);

            // Render parsed tasks in the main area with wrapping and block
            let task_lines: Vec<Line> = tasks
                .iter()
                .map(|task| {
                    // Non-breaking space so strikethrough applies
                    let marker = "•";
                    let title = task.title.replace(" ", "\u{00A0}");
                    let style = if task.completed {
                        Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT)
                    } else {
                        Style::default()
                    };
                    Line::styled(
                        format!("{}{} {}", "\u{00A0}".repeat(task.indent), marker, title),
                        style,
                    )
                })
                .collect();
            let text = Text::from(task_lines);
            let paragraph = Paragraph::new(text)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(" todo.md "));
            frame.render_widget(paragraph, chunks[1]);
        })?;

        // Handle events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if should_quit(&key) {
                    break;
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
