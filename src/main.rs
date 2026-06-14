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
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

const FILE_INDENT: usize = 4;
const RENDER_INDENT: usize = 2;

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
            let indent = (string.len() - trimmed.len()) / FILE_INDENT;
            Some(Task {
                title,
                completed,
                indent,
            })
    }

    fn to_str(&self) -> String {
        let whitespace = " ".repeat(self.indent * FILE_INDENT);
        let marker = if self.completed { "x" } else { " " };
        format!("{}- [{}] {}", whitespace, marker, self.title)
    }

    fn indent(&mut self) {
        self.indent += 1;
    }

    fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    fn toggle_completed(&mut self) {
        self.completed = !self.completed
    }
}

enum InputMode {
    Normal,
    Text,
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
    let mut tasks: Vec<Task> = content
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

    let mut selection = 0;
    let mut text_input = Input::new(String::new());
    let mut input_mode = InputMode::Normal;

    loop {
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
                .enumerate()
                .map(|(i, task)| {
                    // Non-breaking space so strikethrough applies
                    let marker = if task.completed { "✔".green() } else { "•".dark_gray() };
                    let title = task.title.replace(" ", "\u{00A0}");
                    let mut style = Style::default();
                    if task.completed {
                        style = style.fg(Color::DarkGray).crossed_out();
                    }
                    if i == selection {
                        if let InputMode::Text = input_mode {
                            let cursor_x = chunks[1].x + (task.indent * RENDER_INDENT) as u16 + text_input.visual_cursor() as u16 + 3;
                            let cursor_y = chunks[1].y + selection as u16 + 1;
                            frame.set_cursor_position((cursor_x, cursor_y));
                        }
                        else {
                            style = style.bg(Color::Rgb(56, 56, 64));
                        }
                    }
                    Line::from(vec![
                        Span::from("\u{00A0}".repeat(task.indent * RENDER_INDENT)),
                        marker,
                        Span::from(" "),
                        Span::styled(title, style),
                    ])
                })
                .collect();
            let text = Text::from(task_lines);
            let paragraph = Paragraph::new(text)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(" todo.md "));
            frame.render_widget(paragraph, chunks[1]);
        })?;

        // Handle events
        let task = &mut tasks[selection];
        if event::poll(std::time::Duration::MAX)? {
            let event = event::read()?;
            if let Event::Key(key) = event {
                match input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c' | 'd') if key.modifiers == KeyModifiers::CONTROL => break,
                        KeyCode::Char('j') => selection = (selection + 1).min(tasks.len() - 1),
                        KeyCode::Char('k') => selection = selection.saturating_sub(1),
                        KeyCode::Char('x' | ' ') | KeyCode::Enter => task.toggle_completed(),
                        KeyCode::Char('<') => task.dedent(),
                        KeyCode::Char('>') => task.indent(),
                        KeyCode::Char('e' | 'c' | 'a') => {
                            text_input = text_input.with_value(task.title.clone());
                            input_mode = InputMode::Text
                        },
                        KeyCode::Char('i') => {
                            text_input = text_input.with_value(task.title.clone()).with_cursor(0);
                            input_mode = InputMode::Text
                        },
                        _ => {},
                    },
                    InputMode::Text => match key.code {
                        KeyCode::Enter | KeyCode::Esc => input_mode = InputMode::Normal,
                        _ => _ = {
                            text_input.handle_event(&event);
                            task.title = text_input.value().to_string();
                        },
                    }
                }
            }
        }
    }

    // Restore terminal
    restore_terminal();

    Ok(())
}
