use std::fs;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    ExecutableCommand,
};
use ratatui::{
    layout::Constraint,
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

#[derive(Debug, Clone)]
struct Task {
    title: String,
    indent: usize,
    completed: bool,
}

impl Task {
    fn from_str(string: &str) -> Option<Task> {
            let trimmed = string.trim();
            if !trimmed.starts_with("- [") {
                return None;
            }
            let close_bracket = trimmed.find(']')?;
            let checkbox_content = trimmed[3..close_bracket].trim();
            let completed = matches!(checkbox_content, "x");
            let title = trimmed[close_bracket + 1..].trim().to_string();
            if title.is_empty() {
                return None;
            }
            let indent = string.len() - trimmed.len();
            Some(Task {
                title,
                completed,
                indent,
            })
    }

    fn to_str(&self) -> String {
        let whitespace = " ".repeat(self.indent);
        let marker = if self.completed { "x" } else { " " };
        format!("{}- [{}] {}", whitespace, marker, self.title)
    }
}

fn restore_terminal() {
    let mut stdout = std::io::stdout();
    stdout
        .execute(crossterm::terminal::LeaveAlternateScreen)
        .unwrap();
    crossterm::terminal::disable_raw_mode().unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the todo file
    let content = fs::read_to_string("todo.md")?;
    // Trim trailing newline
    let content = content.trim_end();
    // Parse it into Tasks
    let tasks: Vec<Task> = content
        .lines()
        .filter_map(|line| Task::from_str(line))
        .collect();
    // Verify with a round trip test
    let reconstructed_lines: Vec<String> = tasks.iter().map(|task| task.to_str()).collect();
    assert_eq!(content, reconstructed_lines.join("\n"));

    // Disable raw mode on panic
    std::panic::set_hook(Box::new(|info| {
        restore_terminal();
        eprintln!("{info}");
    }));

    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    loop {
        // Handle events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c' | 'd') if key.modifiers == KeyModifiers::CONTROL => break,
                    _ => {},
                }
            }
        }

        // Render
        terminal.draw(|frame| {
            let area = frame.area();

            // Split layout: header takes 2 rows, content takes the rest
            let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(2)]).split(area);

            // Render header block
            let header = Paragraph::new(Line::from(Span::styled(
                " Todos ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )))
            .block(Block::default().borders(Borders::BOTTOM));
            frame.render_widget(header, chunks[0]);

            // Render parsed tasks in the main area
            let task_lines: Vec<Line> = tasks
                .iter()
                .map(|task| {
                    // Non-breaking space so strikethrough applies
                    let marker = "•";
                    let title = task.title.replace(" ", "\u{00A0}");
                    let style = if task.completed {
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::CROSSED_OUT)
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
    }

    // Restore terminal
    restore_terminal();

    Ok(())
}
