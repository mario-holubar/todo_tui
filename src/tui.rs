use std::{error::Error, fs, mem::take};

use ratatui::{
    prelude::*,
    crossterm::{
        cursor::MoveTo,
        event::{self, Event, KeyEvent},
        execute,
    },
    layout::{Constraint, Layout as RatatuiLayout, Rect},
    text::Span,
    widgets::{Block, Borders, Paragraph},
};
use ratatui::widgets::calendar::{CalendarEventStore, Monthly};
use ego_tree::NodeId;
use tui_input::{backend::crossterm::EventHandler, Input};
use time::{Date, Month, OffsetDateTime};

use crate::config::Config;
use crate::tasks::*;

#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Edit,
    DatePicker,
}

#[derive(Debug, Clone, Copy)]
struct DatePickerState {
    is_start_date: bool,
    display_date: Date,
    cursor_date: Date,
}

#[derive(Debug)]
struct HistoryEntry {
    before_content: String,
    after_content: String,
    before_selection_path: Vec<usize>,
    after_selection_path: Vec<usize>,
}

impl DatePickerState {
    fn new(is_start_date: bool, initial_date: Option<Date>) -> Self {
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()).date();
        let display_date = initial_date.unwrap_or(now);
        Self {
            is_start_date,
            display_date,
            cursor_date: display_date,
        }
    }

    fn navigate_month(&mut self, months: i32) {
        let sign = if months > 0 { 1 } else { -1 };
        let mut target_year = self.display_date.year();
        let mut target_month = self.display_date.month() as i32 + months;

        // Normalize month/year
        while target_month > 12 {
            target_month -= 12;
            target_year += sign;
        }
        while target_month < 1 {
            target_month += 12;
            target_year -= sign;
        }

        let new_month = Month::try_from(target_month as u8).unwrap_or(Month::January);
        // Clamp day to valid range for the target month
        let max_day = new_month.length(target_year);
        let day = self.cursor_date.day().min(max_day);

        let new_date = Date::from_calendar_date(target_year, new_month, day).unwrap_or(self.cursor_date);
        self.cursor_date = new_date;
        self.display_date = new_date;
    }

    fn navigate_day(&mut self, delta_days: i32) {
        use time::Duration;
        self.cursor_date = self.cursor_date.checked_add(Duration::days(delta_days as i64)).unwrap_or(self.cursor_date);
        // If cursor moved outside the displayed month, update display
        if self.cursor_date.month() != self.display_date.month()
            || self.cursor_date.year() != self.display_date.year()
        {
            self.display_date = self.cursor_date;
        }
    }
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
    DatePickerConfirm,
    DatePickerCancel,
    DatePickerPrevMonth,
    DatePickerNextMonth,
    DatePickerDayUp,
    DatePickerDayDown,
    DatePickerDayLeft,
    DatePickerDayRight,
    DatePickerClear,
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
    date_picker: Option<DatePickerState>,
    input_mode: InputMode,
    state_changed: bool,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    pending_before_selection_path: Option<Vec<usize>>,
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
            date_picker: None,
            input_mode: InputMode::Normal,
            state_changed: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_before_selection_path: None,
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
        let task = self.tasks.get_task(self.selection);
        let initial_date = if is_start_date {
            task.start_date.as_ref().and_then(|s| Self::parse_date(s))
        } else {
            task.due_date.as_ref().and_then(|s| Self::parse_date(s))
        };
        self.date_picker = Some(DatePickerState::new(is_start_date, initial_date));
        self.input_mode = InputMode::DatePicker;
    }

    fn parse_date(s: &str) -> Option<Date> {
        let bytes = s.as_bytes();
        if bytes.len() != 10 {
            return None;
        }
        let year = std::str::from_utf8(&bytes[0..4]).ok()?.parse::<i32>().ok()?;
        let month_u8 = std::str::from_utf8(&bytes[5..7]).ok()?.parse::<u8>().ok()?;
        let day = std::str::from_utf8(&bytes[8..10]).ok()?.parse::<u8>().ok()?;
        Date::from_calendar_date(year, Month::try_from(month_u8).ok()?, day).ok()
    }

    fn close_date_picker(&mut self) {
        if let Some(ref picker) = self.date_picker {
            let date_str = format!("{}-{:02}-{:02}", picker.cursor_date.year(), picker.cursor_date.month() as u8, picker.cursor_date.day());
            let mut task = self.tasks.get_task(self.selection).clone();
            if picker.is_start_date {
                task.start_date = Some(date_str);
            } else {
                task.due_date = Some(date_str);
            }
            self.tasks.set_task(self.selection, task);
            self.state_changed = true;
        }
        self.date_picker = None;
        self.input_mode = InputMode::Normal;
    }

    fn update_date_picker(&mut self, key_event: KeyEvent) {
        let action = self.config.date_picker_keymap.dispatch(key_event)
            .copied()
            .unwrap_or(Action::NoOp);

        match action {
            Action::DatePickerConfirm => {
                self.close_date_picker();
            }
            Action::DatePickerCancel => {
                self.date_picker = None;
                self.input_mode = InputMode::Normal;
            }
            Action::DatePickerPrevMonth => {
                if let Some(ref mut picker) = self.date_picker {
                    picker.navigate_month(-1);
                }
            }
            Action::DatePickerNextMonth => {
                if let Some(ref mut picker) = self.date_picker {
                    picker.navigate_month(1);
                }
            }
            Action::DatePickerDayUp => {
                if let Some(ref mut picker) = self.date_picker {
                    picker.navigate_day(-7);
                }
            }
            Action::DatePickerDayDown => {
                if let Some(ref mut picker) = self.date_picker {
                    picker.navigate_day(7);
                }
            }
            Action::DatePickerDayLeft => {
                if let Some(ref mut picker) = self.date_picker {
                    picker.navigate_day(-1);
                }
            }
            Action::DatePickerDayRight => {
                if let Some(ref mut picker) = self.date_picker {
                    picker.navigate_day(1);
                }
            }
            Action::DatePickerClear => {
                let mut task = self.tasks.get_task(self.selection).clone();
                if let Some(ref picker) = self.date_picker {
                    if picker.is_start_date {
                        task.start_date = None;
                    } else {
                        task.due_date = None;
                    }
                }
                self.tasks.set_task(self.selection, task);
                self.state_changed = true;
                self.date_picker = None;
                self.input_mode = InputMode::Normal;
            }
            Action::NoOp => {}
            _ => {} // Unhandled actions are ignored
        }
    }

    fn save_change(&mut self, previous_selection_path: Vec<usize>) {
        if self.input_mode != InputMode::Normal || !self.state_changed {
            return;
        }

        let before_content = fs::read_to_string(&self.config.todo_file).unwrap_or_default();
        let after_content = self.tasks.to_string(self.config.file_indent);
        if before_content != after_content {
            let before_selection_path = self.pending_before_selection_path
                .take()
                .unwrap_or(previous_selection_path);
            let after_selection_path = self.tasks.node_to_path(self.selection);
            self.undo_stack.push(HistoryEntry {
                before_content,
                after_content: after_content.clone(),
                before_selection_path,
                after_selection_path,
            });
            self.redo_stack.clear();
            fs::write(&self.config.todo_file, after_content).unwrap();
        } else {
            self.pending_before_selection_path = None;
        }
        self.state_changed = false;
    }

    // Process input. Returns true if the loop should exit.
    fn update(&mut self, key_event: KeyEvent) -> bool {
        let prev_selection_path = self.tasks.node_to_path(self.selection);

        // Handle date picker mode separately
        if self.input_mode == InputMode::DatePicker {
            self.update_date_picker(key_event);
            self.save_change(prev_selection_path);
            return false;
        }

        // Resolve action from the appropriate keymap
        let action = match self.input_mode {
            InputMode::Edit => self.config.text_keymap.dispatch(key_event),
            InputMode::Normal => self.config.normal_keymap.dispatch(key_event),
            InputMode::DatePicker => unreachable!(),
        }.copied()
        .unwrap_or(Action::NoOp);

        if matches!(action,
            Action::Toggle | Action::MovePrev | Action::MoveNext | Action::MoveOut | Action::MoveIn
            | Action::Delete | Action::PasteBelow | Action::PasteAbove
            | Action::AddTop | Action::AddAbove | Action::AddBelow | Action::AddSubtask
        )
            && self.pending_before_selection_path.is_none()
        {
            self.pending_before_selection_path = Some(prev_selection_path.clone());
        }

        let mut should_quit = false;
        match action {
            Action::Quit => {
                if self.input_mode == InputMode::Edit {
                    self.finish_editing();
                }
                should_quit = true;
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
                if let Some(entry) = self.undo_stack.pop() {
                    let (tasks, default_selection) = TaskTree::from_string(&entry.before_content, self.config.file_indent);
                    self.selection = tasks.resolve_path(&entry.before_selection_path).unwrap_or(default_selection);
                    self.tasks = tasks;
                    fs::write(&self.config.todo_file, &entry.before_content).unwrap();
                    self.redo_stack.push(entry);
                }
            }
            Action::Redo => {
                if let Some(entry) = self.redo_stack.pop() {
                    let (tasks, default_selection) = TaskTree::from_string(&entry.after_content, self.config.file_indent);
                    self.selection = tasks.resolve_path(&entry.after_selection_path).unwrap_or(default_selection);
                    self.tasks = tasks;
                    fs::write(&self.config.todo_file, &entry.after_content).unwrap();
                    self.undo_stack.push(entry);
                }
            }
            Action::NoOp if self.input_mode == InputMode::Edit => {
                // Input text
                self.text_input.handle_event(&Event::Key(key_event));
                let mut task = self.tasks.get_task(self.selection).clone();
                task.title = self.text_input.value().to_string();
                self.tasks.set_task(self.selection, task);
            }
            Action::DatePickerConfirm | Action::DatePickerCancel | Action::DatePickerPrevMonth | Action::DatePickerNextMonth
            | Action::DatePickerDayUp | Action::DatePickerDayDown | Action::DatePickerDayLeft | Action::DatePickerDayRight
            | Action::DatePickerClear => {
                // Only handled in date picker mode
            }
            Action::NoOp => {}
        }

        self.save_change(prev_selection_path);

        should_quit
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

        // Draw date picker overlay if active
        if let Some(ref picker) = self.date_picker {
            self.draw_date_picker(frame, area, picker);
        }
    }

    fn draw_date_picker(&self, frame: &mut Frame, area: Rect, picker: &DatePickerState) {
        let label = if picker.is_start_date { "Start date" } else { "Due date" };
        let title = format!(" {} ", label);

        // Center the calendar in the available area
        let cal_width: u16 = 25;
        let cal_height: u16 = 9;
        let popup_width = cal_width.min(area.width.saturating_sub(2));
        let popup_height = cal_height.min(area.height.saturating_sub(2));

        let outer = RatatuiLayout::vertical([
            Constraint::Fill(1),
            Constraint::Length(popup_height),
            Constraint::Fill(1),
        ]);
        let [_top, mid, _bot] = area.layout(&outer);

        let inner = RatatuiLayout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(popup_width),
            Constraint::Fill(1),
        ]);
        let [_left, popup_area, _right] = mid.layout(&inner);

        // Build event store: highlight cursor date and today
        let mut events = CalendarEventStore::default();
        events.add(picker.cursor_date, Style::default().bg(Color::Rgb(36, 36, 42)).bold());

        let calendar = Monthly::new(picker.display_date, events)
            .show_month_header(Style::default().bold())
            .show_weekdays_header(Style::default().fg(Color::Rgb(140, 140, 160)))
            .block(
                Block::bordered()
                    .title(Span::styled(title, Style::default().bold())),
            );

        frame.render_widget(calendar, popup_area);
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
