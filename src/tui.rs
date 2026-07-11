use std::{error::Error, fs, mem::take};

use ratatui::{
    prelude::*,
    crossterm::{
        cursor::MoveTo,
        event::{self, Event, KeyEvent},
        execute,
    },
    widgets::{Block, Borders, List, ListItem, ListState},
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
    AddTop,
    AddAbove,
    AddBelow,
    AddSubtask,
    Edit,
    EditBeginning,
    EditDone,
    NoOp,
}

#[derive(Debug)]
pub struct Tui {
    config: Config,
    tasks: TaskTree,
    selection: NodeId,
    text_input: Input,
    input_mode: InputMode,
    state_changed: bool,
    list_state: ListState,
    // TODO Does state_changed need to be a field?
}

impl Tui {
    pub fn new() -> Tui {
        let config = Config::load().unwrap();

        // Read the todo file
        let content = match fs::read_to_string(&config.todo_file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "".to_string(),
            Err(e) => panic!("Failed to read todo file: {e}"),
        };
        // Parse it into Tasks
        let (tasks, selection) = TaskTree::from_string(&content, config.file_indent);
        // Verify with a round trip test
        assert_eq!(content, tasks.to_string(config.file_indent));

        let list_state = ListState::default().with_selected(Some(0));
        Tui {
            config,
            tasks,
            selection,
            text_input: Input::new(String::new()),
            input_mode: InputMode::Normal,
            state_changed: false,
            list_state,
        }
    }

    fn save_todos(&self) {
        // Serialize todos
        let content = self.tasks.to_string(self.config.file_indent);
        // Verify with a round trip test
        let (_reconstructed_tasks, _) = TaskTree::from_string(&content, self.config.file_indent);
        // TODO Equality of Trees is strict (includes IDs). Need to walk the trees to verify
        //assert_eq!(content, reconstructed_tasks.to_string(self.config.file_indent));
        // Save to file
        fs::write(&self.config.todo_file, content).unwrap();
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

    // Process input. Returns true if the loop should exit.
    fn update(&mut self, key_event: KeyEvent) -> bool {
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
            Action::EditDone => {
                self.finish_editing();
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
            Action::EditBeginning => {
                self.text_input = take(&mut self.text_input)
                    .with_value(self.tasks.get_task(self.selection).title.clone())
                    .with_cursor(0);
                self.input_mode = InputMode::Edit;
            }
            Action::Delete => {
                self.selection = self.tasks.remove(self.selection);
                self.state_changed = true;
            }
            Action::NoOp if self.input_mode == InputMode::Edit => {
                // Input text
                self.text_input.handle_event(&Event::Key(key_event));
                let mut task = self.tasks.get_task(self.selection).clone();
                task.title = self.text_input.value().to_string();
                self.tasks.set_task(self.selection, task);
            }
            _ => {}
        }

        // Save state if changed
        if self.input_mode == InputMode::Normal && self.state_changed {
            self.save_todos();
            self.state_changed = false;
        }

        false
    }

    fn cursor_position(&self) -> (u16, u16) {
        let all_ids = self.tasks.all_ids();
        let selected_idx = all_ids.iter().position(|&id| id == self.selection).unwrap_or(0);
        // Row: 1 (title bar / top border) + selected task index
        let row = 1 + selected_idx as u16;
        // Column: 1 (left border) + 2 ("> ") + indent prefix + 1 (marker) + 1 (space after marker)
        let node = self.tasks.get_node(self.selection);
        let indent = node.ancestors().count() - 1;
        let col: u16 = (1 + 2 + indent * self.config.display_indent + 1 + 1
            + self.text_input.cursor()) as u16;
        (col, row)
    }

    fn sync_selection(&mut self) {
        let all_ids = self.tasks.all_ids();
        let selected_idx = all_ids.iter().position(|&id| id == self.selection).unwrap_or(0);
        if self.list_state.selected() != Some(selected_idx) {
            self.list_state.select(Some(selected_idx));
        }
    }

    fn draw_list(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let lines = self.tasks.display(self.config.display_indent);
        let items: Vec<ListItem> = lines.into_iter().map(ListItem::new).collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", self.config.todo_file)),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::Rgb(56, 56, 64)));

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    pub fn main(&mut self) -> Result<(), Box<dyn Error>> {
        ratatui::run(|terminal| {
            self.sync_selection();
            terminal.draw(|frame| self.draw_list(frame))?;

            loop {
                if event::poll(std::time::Duration::MAX)? {
                    let event = event::read()?;
                    if let Event::Key(key) = event {
                        if self.update(key) {
                            break;
                        }
                    }
                }
                self.sync_selection();
                terminal.draw(|frame| self.draw_list(frame))?;

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
