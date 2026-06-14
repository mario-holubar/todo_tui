use std::{error::Error, fs, io::Stdout, mem::take};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Constraint,
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::config::*;
use crate::tasks::*;

#[derive(Debug, Default)]
enum InputMode {
    #[default]
    Normal,
    Text,
}

#[derive(Debug, Default)]
pub struct Tui {
    tasks: Vec<Task>,
    selection: Option<usize>,
    text_input: Input,
    input_mode: InputMode,
}

impl Tui {
    pub fn new() -> Tui {
        let mut tui = Tui::default();
        tui.load_todos();
        if !tui.tasks.is_empty() {
            tui.selection = Some(0);
        }
        tui
    }

    fn load_todos(&mut self) {
        // Read the todo file
        let content = match fs::read_to_string(TODO_FILE) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "".to_string(),
            Err(e) => panic!("Failed to read file: {e}"),
        };
        // Trim trailing newline
        let content = content.trim_end();
        // Parse it into Tasks
        self.tasks = content.lines().filter_map(Task::from_str).collect();
        // Verify with a round trip test
        let reconstructed_lines: Vec<String> =
            self.tasks.iter().map(|task| task.to_str()).collect();
        assert_eq!(content, reconstructed_lines.join("\n"));
    }

    // Process input. Returns true if the loop should exit
    fn update(&mut self, key_event: KeyEvent) -> bool {
        match self.input_mode {
            InputMode::Normal => {
                // Normal mode hotkeys
                match key_event.code {
                    KeyCode::Char('q') => {
                        return true;
                    }
                    KeyCode::Char('n') => {
                        self.tasks.insert(0, Task::default());
                        self.selection = Some(0);
                        self.text_input = Input::new("".to_string());
                        self.input_mode = InputMode::Text;
                    }
                    _ => {}
                }
                // Normal mode hotkeys if selection active
                if let Some(idx) = self.selection {
                    match key_event.code {
                        KeyCode::Char('j') => {
                            self.selection = Some((idx + 1).min(self.tasks.len() - 1))
                        }
                        KeyCode::Char('k') => {
                            self.selection = Some(idx.saturating_sub(1))
                        }
                        KeyCode::Char('x' | ' ') | KeyCode::Enter => {
                            self.tasks[idx].toggle_completed()
                        }
                        KeyCode::Char('<') => {
                            self.tasks[idx].dedent()
                        }
                        KeyCode::Char('>') => {
                            self.tasks[idx].indent()
                        }
                        KeyCode::Char('e' | 'c' | 'a') => {
                            self.text_input = take(&mut self.text_input).with_value(self.tasks[idx].title.clone());
                            self.input_mode = InputMode::Text;
                        }
                        KeyCode::Char('i') => {
                            self.text_input = take(&mut self.text_input)
                                .with_value(self.tasks[idx].title.clone())
                                .with_cursor(0);
                            self.input_mode = InputMode::Text;
                        }
                        KeyCode::Char('d') => {
                            self.tasks.remove(idx);
                            if self.tasks.is_empty() {
                                self.selection = None;
                            }
                            else {
                                self.selection = Some(idx.min(self.tasks.len() - 1));
                            }
                        }
                        KeyCode::Char('o') => {
                            let new_task = Task {
                                indent: self.tasks[idx].indent,
                                ..Default::default()
                            };
                            self.tasks.insert(idx + 1, new_task);
                            self.selection = Some(idx + 1);
                            self.text_input = Input::new("".to_string());
                            self.input_mode = InputMode::Text;
                        }
                        KeyCode::Char('O') => {
                            let new_task = Task {
                                indent: self.tasks[idx].indent,
                                ..Default::default()
                            };
                            self.tasks.insert(idx, new_task);
                            self.text_input = Input::new("".to_string());
                            self.input_mode = InputMode::Text;
                        }
                        _ => {}
                    }
                }
            },
            InputMode::Text => match key_event.code {
                // Insert mode
                KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
                _ => {
                    self.text_input.handle_event(&Event::Key(key_event));
                    self.tasks[self.selection.unwrap()].title = self.text_input.value().to_string();
                }
            },
        }
        false
    }

    fn draw(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), Box<dyn Error>> {
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
            let task_lines: Vec<Line> = self
                .tasks
                .iter()
                .enumerate()
                .map(|(i, task)| {
                    let marker = if task.completed {
                        "✔".green()
                    } else {
                        "•".dark_gray()
                    };

                    // Non-breaking space so strikethrough applies
                    let title = task.title.replace(" ", "\u{00A0}");

                    let mut style = Style::default();
                    if task.completed {
                        style = style.fg(Color::DarkGray).crossed_out();
                    }
                    if self.selection == Some(i) {
                        if let InputMode::Text = self.input_mode {
                            let cursor_x = chunks[1].x
                                + (task.indent * RENDER_INDENT) as u16
                                + self.text_input.visual_cursor() as u16
                                + 3;
                            let cursor_y = chunks[1].y + i as u16 + 1;
                            frame.set_cursor_position((cursor_x, cursor_y));
                        } else {
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
        Ok(())
    }

    pub fn main(&mut self) -> Result<(), Box<dyn Error>> {
        ratatui::run(|terminal| {
            self.draw(terminal)?;
            loop {
                if event::poll(std::time::Duration::MAX)? {
                    let event = event::read()?;
                    if let Event::Key(key) = event {
                        match key.code {
                            KeyCode::Char('c' | 'd') if key.modifiers == KeyModifiers::CONTROL => {
                                break;
                            }
                            _ => {
                                if self.update(key) {
                                    break;
                                }
                            }
                        }
                    }
                }
                self.draw(terminal)?;
            }
            Ok(())
        })
    }
}
