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

    fn serialize(&self) -> String {
        self.tasks.iter().fold(String::new(), |mut acc, task| {
            let line = task.to_str() + "\n";
            acc.push_str(&line);
            acc
        })
    }

    fn load_todos(&mut self) {
        // Read the todo file
        let content = match fs::read_to_string(TODO_FILE) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "".to_string(),
            Err(e) => panic!("Failed to read todo file: {e}"),
        };
        // Parse it into Tasks
        self.tasks = content.lines().filter_map(Task::from_str).collect();
        // Verify with a round trip test
        assert_eq!(content, self.serialize());
    }

    fn save_todos(&self) {
        // Serialize todos
        let content = self.serialize();
        // Verify with a round trip test
        let reconstructed_tasks: Vec<Task> = content.lines().filter_map(Task::from_str).collect();
        assert_eq!(self.tasks, reconstructed_tasks);
        // Save to file
        fs::write(TODO_FILE, content).unwrap();
    }

    fn get_parent(&self, idx: usize) -> Option<usize> {
        if idx == 0 || self.tasks[idx].indent == 0 {
            return None;
        } else {
            for i in (0..idx).rev() {
                if self.tasks[i].indent == self.tasks[idx].indent - 1 {
                    return Some(i);
                }
            }
        }
        None
    }

    fn get_children(&self, idx: usize) -> Vec<usize> {
        let task = &self.tasks[idx];
        let mut children = vec![];
        for i in (idx + 1)..self.tasks.len() {
            let t = &self.tasks[i];
            if t.indent == task.indent + 1 {
                children.push(i);
            } else if t.indent <= task.indent {
                break;
            }
        }
        children
    }

    fn get_siblings(&self, idx: usize) -> Vec<usize> {
        let task = &self.tasks[idx];
        let mut siblings = vec![];
        for i in (0..idx).rev() {
            let t = &self.tasks[i];
            if t.indent == task.indent {
                siblings.push(i);
            } else if t.indent <= task.indent {
                break;
            }
        }
        for i in (idx + 1)..(self.tasks.len() - 1) {
            let t = &self.tasks[i];
            if t.indent == task.indent {
                siblings.push(i);
            } else if t.indent <= task.indent {
                break;
            }
        }
        siblings
    }

    fn next_sibling(&self, idx: usize) -> Option<usize> {
        let task = &self.tasks[idx];
        for i in (idx + 1)..self.tasks.len() {
            let t = &self.tasks[i];
            if t.indent == task.indent {
                return Some(i);
            } else if t.indent <= task.indent {
                break;
            }
        }
        None
    }

    fn prev_sibling(&self, idx: usize) -> Option<usize> {
        let task = &self.tasks[idx];
        for i in (0..idx).rev() {
            let t = &self.tasks[i];
            if t.indent == task.indent {
                return Some(i);
            } else if t.indent <= task.indent {
                break;
            }
        }
        None
    }

    fn end_of_children(&self, idx: usize) -> usize {
        if !self.has_children(idx) {
            return idx;
        };
        self.end_of_children(*self.get_children(idx).last().unwrap())
    }

    fn has_children(&self, idx: usize) -> bool {
        self.tasks.len() > idx + 1 && self.tasks[idx + 1].indent > self.tasks[idx].indent
    }

    fn is_first_child(&self, idx: usize) -> bool {
        idx == 0 || self.tasks[idx - 1].indent < self.tasks[idx].indent
    }

    fn is_first_actionable(&self, idx: usize) -> bool {
        if self.tasks[idx].indent == 0 {
            return true;
        }
        let Some(parent) = self.get_parent(idx) else {
            return true;
        };
        self.is_first_actionable(parent)
            && !self
                .get_siblings(idx)
                .iter()
                .any(|&i| i < idx && !self.tasks[i].completed)
    }

    fn update_parent_completion(&mut self, idx: usize) {
        if !self.has_children(idx) {
            return;
        };
        let completed = self
            .get_children(idx)
            .iter()
            .all(|&i| self.tasks[i].completed);
        if completed != self.tasks[idx].completed {
            self.tasks[idx].completed = completed;
            if let Some(parent) = self.get_parent(idx) {
                self.update_parent_completion(parent);
            }
        }
    }

    fn set_children_completion(&mut self, idx: usize) {
        self.get_children(idx).iter().for_each(|&i| {
            self.tasks[i].completed = self.tasks[idx].completed;
            self.set_children_completion(i);
        });
    }

    fn toggle_completed(&mut self, idx: usize) {
        self.tasks[idx].toggle_completed();
        if let Some(parent) = self.get_parent(idx) {
            self.update_parent_completion(parent);
        }
        self.set_children_completion(idx);
    }

    // Returns new index of task
    fn transpose_up(&mut self, idx: usize) -> usize {
        let prev = match self.prev_sibling(idx) {
            Some(i) => i,
            None => return idx,
        };
        let end_of_prev = self.end_of_children(prev);
        let end_of_task = self.end_of_children(idx);

        // Swap the current task and its previous sibling (including children)
        let size_of_prev = end_of_prev + 1 - prev;
        let size_of_task = end_of_task + 1 - idx;
        self.tasks[prev..=end_of_task].rotate_right(size_of_task);

        idx - size_of_prev
    }

    // Returns new index of task
    fn transpose_down(&mut self, idx: usize) -> usize {
        let next = match self.next_sibling(idx) {
            Some(i) => i,
            None => return idx,
        };
        let end_of_task = self.end_of_children(idx);
        let end_of_next = self.end_of_children(next);

        // Swap the current task and its next sibling (including children)
        let size_of_task = end_of_task + 1 - idx;
        let size_of_next = end_of_next + 1 - next;
        self.tasks[idx..=end_of_next].rotate_left(size_of_task);

        idx + size_of_next
    }

    // Returns new index of task
    fn promote(&mut self, mut idx: usize) -> usize {
        if self.tasks[idx].indent == 0 {
            return idx;
        };

        let parent = self.get_parent(idx);
        let mut end_of_task = self.end_of_children(idx);

        // Move task and children to after last sibling
        if let Some(p) = parent {
            let last_sibling = *self.get_children(p).last().unwrap();
            let end_of_parent = self.end_of_children(last_sibling);
            let size_of_task = end_of_task + 1 - idx;
            self.tasks[idx..=end_of_parent].rotate_left(size_of_task);
            idx += end_of_parent - end_of_task;
            end_of_task = end_of_parent;
        }

        for i in idx..=end_of_task {
            self.tasks[i].dedent()
        }

        if let Some(p) = parent {
            self.update_parent_completion(p);
        }

        idx
    }

    fn demote(&mut self, idx: usize) {
        self.get_children(idx).iter().for_each(|&i| self.demote(i));
        self.tasks[idx].indent();

        if let Some(p) = self.get_parent(idx) {
            self.update_parent_completion(p);
        }
    }

    // TODO Undo/redo
    // Process input. Returns true if the loop should exit
    fn update(&mut self, key_event: KeyEvent) -> bool {
        let state_changed = match self.input_mode {
            InputMode::Normal => {
                // Normal mode
                let state_changed = match key_event.code {
                    KeyCode::Char('q') => {
                        return true;
                    }
                    KeyCode::Char('n') => {
                        self.tasks.insert(0, Task::default());
                        self.selection = Some(0);
                        self.text_input = Input::new("".to_string());
                        self.input_mode = InputMode::Text;
                        true
                    }
                    _ => {
                        false
                    }
                };
                // Normal mode, selection active
                state_changed || if let Some(idx) = self.selection {
                    match key_event.modifiers {
                        KeyModifiers::NONE => match key_event.code {
                            KeyCode::Char('x' | ' ') | KeyCode::Enter => {
                                self.toggle_completed(idx);
                                true
                            }
                            KeyCode::Char('j') => {
                                self.selection = Some((idx + 1).min(self.tasks.len() - 1));
                                false
                            }
                            KeyCode::Char('k') => {
                                self.selection = Some(idx.saturating_sub(1));
                                false
                            },
                            KeyCode::Char('h') => {
                                if let Some(p) = self.get_parent(idx) {
                                    self.selection = Some(p);
                                }
                                false
                            }
                            KeyCode::Char('l') => {
                                if let Some(&c) = self.get_children(idx).first() {
                                    self.selection = Some(c);
                                }
                                false
                            }
                            KeyCode::Char('<') | KeyCode::BackTab => {
                                self.selection = Some(self.promote(idx));
                                true
                            }
                            KeyCode::Char('>') | KeyCode::Tab =>
                            {
                                #[allow(clippy::collapsible_match)]
                                if !self.is_first_child(idx) {
                                    self.demote(idx);
                                }
                                true
                            }
                            KeyCode::Char('e' | 'c' | 'a') => {
                                self.text_input = take(&mut self.text_input)
                                    .with_value(self.tasks[idx].title.clone());
                                self.input_mode = InputMode::Text;
                                false
                            }
                            KeyCode::Char('i') => {
                                self.text_input = take(&mut self.text_input)
                                    .with_value(self.tasks[idx].title.clone())
                                    .with_cursor(0);
                                self.input_mode = InputMode::Text;
                                false
                            }
                            KeyCode::Char('d') => {
                                self.tasks.remove(idx);
                                if self.tasks.is_empty() {
                                    self.selection = None;
                                } else {
                                    self.selection = Some(idx.min(self.tasks.len() - 1));
                                }
                                true
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
                                false
                            }
                            KeyCode::Char('O') => {
                                let new_task = Task {
                                    indent: self.tasks[idx].indent,
                                    ..Default::default()
                                };
                                self.tasks.insert(idx, new_task);
                                self.text_input = Input::new("".to_string());
                                self.input_mode = InputMode::Text;
                                false
                            }
                            KeyCode::Char('s') => {
                                let new_task = Task {
                                    indent: self.tasks[idx].indent + 1,
                                    ..Default::default()
                                };
                                self.tasks.insert(idx + 1, new_task);
                                self.selection = Some(idx + 1);
                                self.text_input = Input::new("".to_string());
                                self.input_mode = InputMode::Text;
                                false
                            }
                            _ => {
                                false
                            }
                        },
                        KeyModifiers::CONTROL => match key_event.code {
                            KeyCode::Char('j') => {
                                self.selection = Some(self.transpose_down(idx));
                                true
                            }
                            KeyCode::Char('k') => {
                                self.selection = Some(self.transpose_up(idx));
                                true
                            }
                            KeyCode::Char('h') => {
                                self.selection = Some(self.promote(idx));
                                true
                            }
                            KeyCode::Char('l') =>
                            {
                                #[allow(clippy::collapsible_match)]
                                if !self.is_first_child(idx) {
                                    self.demote(idx);
                                }
                                true
                            }
                            _ => {
                                false
                            }
                        },
                        _ => {
                            false
                        }
                    }
                }
                else {
                    false
                }
            }
            InputMode::Text => match key_event.code {
                // Insert mode
                KeyCode::Enter | KeyCode::Esc => {
                    let idx = self.selection.as_mut().unwrap();
                    if self.tasks[*idx].title.is_empty() {
                        self.tasks.remove(*idx);
                        if self.tasks.is_empty() {
                            self.selection = None;
                        }
                        else {
                            *idx = idx.saturating_sub(1);
                        }
                    }
                    self.input_mode = InputMode::Normal;
                    true
                }
                KeyCode::Tab => {
                    self.tasks[self.selection.unwrap()].indent();
                    false
                }
                KeyCode::BackTab => {
                    self.tasks[self.selection.unwrap()].dedent();
                    false
                }
                _ => {
                    self.text_input.handle_event(&Event::Key(key_event));
                    self.tasks[self.selection.unwrap()].title = self.text_input.value().to_string();
                    false
                }
            },
        };

        if let InputMode::Normal = self.input_mode {
            if state_changed {
                self.save_todos();
            }
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
                    let mut marker = "◯".reset();
                    let mut title = task.title.replace(" ", "\u{00A0}").reset(); // Non-breaking space so strikethrough applies

                    if task.completed {
                        marker = "◉".fg(Color::DarkGray).dim();
                        title = title.fg(Color::DarkGray).dim();
                    } else if self.has_children(i) {
                        //marker = "▷".dim();
                        marker = marker.reset();
                        title = title.reset();
                    } else if self.is_first_actionable(i) {
                        marker = marker.green();
                        title = title.green().bold();
                    } else {
                        marker = marker.dim();
                        title = title.dim();
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
                            title = title.bg(Color::Rgb(56, 56, 64));
                        }
                    }

                    let prefix = "\u{00A0}".repeat(task.indent * RENDER_INDENT);
                    Line::from(vec![prefix.dark_gray(), marker, Span::from(" "), title])
                })
                .collect();

            let text = Text::from(task_lines);
            let paragraph = Paragraph::new(text).wrap(Wrap { trim: true }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", TODO_FILE)),
            );
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
