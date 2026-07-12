use std::{error::Error, fs, mem::take};

use ratatui::{
    prelude::*,
    crossterm::{
        cursor::MoveTo,
        event::{self, Event, KeyEvent},
        execute,
    },
    widgets::{Block, Borders, Paragraph},
};
use ego_tree::NodeId;
use tui_input::{backend::crossterm::EventHandler, Input};

use crate::config::Config;
use crate::tasks::*;

#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Edit,
}

// TODO Need separate edit mode actions, or handle edit mode for all cases
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub enum Action {
    Quit,
    Toggle,
    SelectionUp,
    SelectionDown,
    SelectionPrev,
    SelectionNext,
    SelectionOut,
    SelectionIn,
    MoveUp,
    MoveDown,
    MovePrev,
    MoveNext,
    MoveOut,
    MoveIn,
    Delete,
    Copy,
    PasteBelow,
    PasteAbove,
    AddTop,
    AddAbove,
    AddBelow,
    AddSubtask,
    Edit,
    EditBeginning,
    EditClear,
    EditDone,
    SetStartDate,
    SetDueDate,
    Undo,
    Redo,
    NoOp,
}

#[derive(Debug)]
pub struct Tui {
    config: Config,
    tasks: TaskTree,
    selection: NodeId,
    text_input: Input,
    clipboard: Option<String>,
    input_mode: InputMode,
    state_changed: bool,
    undo_stack: Vec<(String, Vec<usize>)>,
    redo_stack: Vec<(String, Vec<usize>)>,
}

impl Tui {
    pub fn new() -> Tui {
        let config = Config::load().unwrap();

        // Read the todo file
        let content = match fs::read_to_string(&config.todo_file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "\n".to_string(),
            Err(e) => panic!("Failed to read todo file: {e}"),
        };
        // Parse it into Tasks
        let (tasks, selection) = TaskTree::from_string(&content, config.file_indent);
        // Verify with a round trip test
        assert_eq!(content, tasks.to_string(config.file_indent));

        Tui {
            config,
            tasks,
            selection,
            text_input: Input::new(String::new()),
            clipboard: None,
            input_mode: InputMode::Normal,
            state_changed: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    fn copy_selection_to_clipboard(&mut self) {
        let mut lines = Vec::new();
        self.tasks.serialize_node(&mut lines, self.tasks.get_node(self.selection), self.config.file_indent);
        self.clipboard = Some(lines.join("\n") + "\n");
    }

    fn begin_editing(&mut self) {
        self.text_input = take(&mut self.text_input)
            .with_value(self.tasks.get_task(self.selection).title.clone());
        self.input_mode = InputMode::Edit;
    }

    // Returns new selection index
    fn finish_editing(&mut self) {
        if self.input_mode != InputMode::Edit { return; }
        self.input_mode = InputMode::Normal;
        let mut title = self.tasks.get_task(self.selection).title.clone();
        title = title.trim().to_string();
        if title.is_empty() {
            self.selection = self.tasks.remove(self.selection);
        }
        else {
            self.tasks.set_title(self.selection, title);
            self.state_changed = true;
        };
    }

    fn open_date_picker(&mut self, is_start_date: bool) {
        let label = if is_start_date { "Start date" } else { "Due date" };
        println!("Opening {} picker for task: {}", label, self.tasks.get_task(self.selection).title);
        // TODO: Implement date picker overlay using ratatui_widgets::calendar::Monthly
    }

    // Process input. Returns true if the loop should exit.
    fn update(&mut self, key_event: KeyEvent) -> bool {
        let prev_selection_path = self.tasks.node_to_path(self.selection);

        // Resolve action from the appropriate keymap
        let action = match self.input_mode {
            InputMode::Edit => self.config.text_keymap.dispatch(key_event),
            InputMode::Normal => self.config.normal_keymap.dispatch(key_event),
        }.copied()
        .unwrap_or(Action::NoOp);

        match action {
            Action::Quit => {
                if self.input_mode == InputMode::Edit {
                    self.finish_editing();
                }
                return true;
            }
            Action::Toggle => {
                self.tasks.toggle_completed(self.selection);
                self.state_changed = true;
            }
            Action::SelectionUp => {
                if let Some(parent) = self.tasks.get_above(self.selection) {
                    self.selection = parent;
                }
            },
            Action::SelectionDown => {
                if let Some(parent) = self.tasks.get_below(self.selection) {
                    self.selection = parent;
                }
            },
            Action::SelectionPrev => {
                if let Some(parent) = self.tasks.get_prev_sibling(self.selection) {
                    self.selection = parent;
                }
            },
            Action::SelectionNext => {
                if let Some(parent) = self.tasks.get_next_sibling(self.selection) {
                    self.selection = parent;
                }
            },
            Action::SelectionIn => {
                if let Some(child) = self.tasks.get_first_child(self.selection) {
                    self.selection = child;
                }
            }
            Action::SelectionOut => {
                if let Some(parent) = self.tasks.get_parent_non_root(self.selection) {
                    self.selection = parent;
                }
            }
            Action::MoveUp => {
                todo!(); // TODO
            }
            Action::MoveDown => {
                todo!(); // TODO
            }
            Action::MovePrev => {
                if let Some(id) = self.tasks.switch_with_prev_sibling(self.selection) {
                    self.selection = id;
                }
                self.state_changed = true;
            }
            Action::MoveNext => {
                if let Some(id) = self.tasks.switch_with_next_sibling(self.selection) {
                    self.selection = id;
                }
                self.state_changed = true;
            }
            Action::MoveOut => {
                if let Some(id) = self.tasks.move_out(self.selection) {
                    self.selection = id;
                }
                self.state_changed = true;
            }
            Action::MoveIn => {
                if let Some(id) = self.tasks.move_in(self.selection) {
                    self.selection = id;
                }
                self.state_changed = true;
            }
            Action::Edit => {
                self.begin_editing()
            },
            Action::EditBeginning => {
                self.text_input = take(&mut self.text_input)
                    .with_value(self.tasks.get_task(self.selection).title.clone())
                    .with_cursor(0);
                self.input_mode = InputMode::Edit;
            }
            Action::EditClear => {
                self.tasks.set_title(self.selection, String::new());
                self.begin_editing()
            }
            Action::EditDone => {
                self.finish_editing();
            }
            Action::SetStartDate => {
                self.open_date_picker(true);
            }
            Action::SetDueDate => {
                self.open_date_picker(false);
            }
            Action::AddTop => {
                self.selection = self.tasks.add_top_level();
                self.begin_editing();
                self.state_changed = true;
            }
            Action::AddAbove => {
                self.selection = self.tasks.add_sibling_above(self.selection);
                self.begin_editing();
                self.state_changed = true;
            }
            Action::AddBelow => {
                self.selection = self.tasks.add_sibling_below(self.selection);
                self.begin_editing();
                self.state_changed = true;
            }
            Action::AddSubtask => {
                self.selection = self.tasks.add_child(self.selection);
                self.begin_editing();
                self.state_changed = true;
            }
            Action::Delete => {
                self.copy_selection_to_clipboard();
                self.selection = self.tasks.remove(self.selection);
                self.state_changed = true;
            }
            Action::Copy => {
                self.copy_selection_to_clipboard();
            }
            Action::PasteBelow => {
                if let Some(ref content) = self.clipboard {
                    self.selection = self.tasks.paste_branch_below(self.selection, content);
                    self.state_changed = true;
                }
            }
            Action::PasteAbove => {
                if let Some(ref content) = self.clipboard {
                    self.selection = self.tasks.paste_branch_above(self.selection, content);
                    self.state_changed = true;
                }
            }
            Action::Undo => {
                if let Some((prev_content, selection_path)) = self.undo_stack.pop() {
                    let current_content = fs::read_to_string(&self.config.todo_file)
                        .unwrap_or_default();
                    self.redo_stack.push((current_content, selection_path.clone()));
                    let (tasks, default_selection) = TaskTree::from_string(&prev_content, self.config.file_indent);
                    self.selection = tasks.resolve_path(&selection_path).unwrap_or(default_selection);
                    self.tasks = tasks;
                    fs::write(&self.config.todo_file, &prev_content).unwrap();
                }
            }
            Action::Redo => {
                if let Some((next_content, selection_path)) = self.redo_stack.pop() {
                    let current_content = fs::read_to_string(&self.config.todo_file)
                        .unwrap_or_default();
                    self.undo_stack.push((current_content, selection_path.clone()));
                    let (tasks, default_selection) = TaskTree::from_string(&next_content, self.config.file_indent);
                    self.selection = tasks.resolve_path(&selection_path).unwrap_or(default_selection);
                    self.tasks = tasks;
                    fs::write(&self.config.todo_file, &next_content).unwrap();
                }
            }
            Action::NoOp if self.input_mode == InputMode::Edit => {
                // Input text
                self.text_input.handle_event(&Event::Key(key_event));
                let mut task = self.tasks.get_task(self.selection).clone();
                task.title = self.text_input.value().to_string();
                self.tasks.set_task(self.selection, task);
            }
            Action::NoOp => {}
        }

        // Save state if changed
        if self.input_mode == InputMode::Normal && self.state_changed {
            let old_content = fs::read_to_string(&self.config.todo_file)
                .unwrap_or_default();
            if self.undo_stack.last() != Some(&(old_content.clone(), prev_selection_path.clone())) {
                self.undo_stack.push((old_content, prev_selection_path));
            }
            self.redo_stack.clear();

            let content = self.tasks.to_string(self.config.file_indent);
            let (_reconstructed_tasks, _) = TaskTree::from_string(&content, self.config.file_indent);
            fs::write(&self.config.todo_file, content).unwrap();
            self.state_changed = false;
        }

        false
    }

    fn cursor_position(&self) -> (u16, u16) {
        let all_ids = self.tasks.all_ids();
        let selected_idx = all_ids.iter().position(|&id| id == self.selection).unwrap_or(0);
        // Row: 1 (title bar / top border) + selected task index
        let row = 1 + selected_idx as u16;
        // Column: 1 (left border) + indent prefix + 1 (marker) + 1 (space after marker)
        let node = self.tasks.get_node(self.selection);
        let indent = node.ancestors().count() - 1;
        let col: u16 = (1 + indent * self.config.display_indent + 1 + 1
            + self.text_input.cursor()) as u16;
        (col, row)
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        let inner_width = (area.width - 2) as usize; // subtract borders
        let lines = self.tasks.display(self.config.display_indent, self.selection, inner_width);
        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", self.config.todo_file)),
        );
        frame.render_widget(paragraph, area);
    }

    pub fn main(&mut self) -> Result<(), Box<dyn Error>> {
        ratatui::run(|terminal| {
            terminal.draw(|frame| self.draw(frame))?;

            loop {
                if event::poll(std::time::Duration::MAX)? {
                    let event = event::read()?;
                    if let Event::Key(key) = event {
                        if self.update(key) {
                            break;
                        }
                    }
                }
                terminal.draw(|frame| self.draw(frame))?;

                if self.input_mode == InputMode::Edit {
                    let (col, row) = self.cursor_position();
                    use std::io::stdout;
                    execute!(stdout(), MoveTo(col, row))?;
                    terminal.show_cursor()?;
                } else {
                    terminal.hide_cursor()?;
                }
            }
            Ok(())
        })
    }
}
