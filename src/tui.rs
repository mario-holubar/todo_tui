use std::{error::Error, fs, mem::{take, swap}};

use ratatui::{
    prelude::*,
    crossterm::{
        cursor::MoveTo,
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute,
    },
    layout::{Constraint, Layout as RatatuiLayout, Rect},
    text::Span,
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui::widgets::calendar::{CalendarEventStore, Monthly};
use ego_tree::NodeId;
use tui_input::{backend::crossterm::EventHandler, Input};
use time::{Date, Month, OffsetDateTime};

use crate::config::Config;
use crate::document::{self, Tab};
use crate::tasks::*;

#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Edit,
    EditTab,
    DatePicker,
    Search,
    QuitConfirm,
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
    before_tab: usize,
    after_tab: usize,
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
    SelectionFirst,
    SelectionLast,
    SelectionOut,
    SelectionIn,
    // MoveUp,
    // MoveDown,
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
    Cancel,
    TabNext,
    TabPrev,
    TabRename,
    TabAdd,
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
    SearchForward,
    SearchBackward,
    NoOp,
}

#[derive(Debug)]
pub struct Tui {
    config: Config,
    tasks: TaskTree,
    selection: NodeId,
    scroll_offset: usize,
    tabs: Vec<Tab>,
    active_tab: usize,
    tab_before_edit: Option<String>,
    tab_edit_previous: usize,
    text_input: Input,
    clipboard: Option<String>,
    date_picker: Option<DatePickerState>,
    search_query: String,
    search_origin: Option<NodeId>,
    search_forward: bool,
    input_mode: InputMode,
    state_changed: bool,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    pending_before_selection_path: Option<Vec<usize>>,
    pending_before_tab: Option<usize>,
    history_content: String,
    saved_content: String,
}

impl Tui {
    fn serialize(&self) -> String {
        self.tabs.iter().enumerate().map(|(index, tab)| {
            let tasks = if index == self.active_tab { &self.tasks } else { &tab.tasks };
            document::serialize_section(&tab.name, tasks, self.config.general.file_indent)
        }).collect::<Vec<_>>().join("\n")
    }

    fn switch_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active_tab { return; }
        swap(&mut self.tasks, &mut self.tabs[self.active_tab].tasks);
        swap(&mut self.selection, &mut self.tabs[self.active_tab].selection);
        self.active_tab = index;
        swap(&mut self.tasks, &mut self.tabs[index].tasks);
        swap(&mut self.selection, &mut self.tabs[index].selection);
        self.scroll_offset = 0;
    }

    fn move_tab(&mut self, right: bool) {
        let len = self.tabs.len();
        if len < 2 { return; }
        let target = if right { (self.active_tab + 1) % len } else { (self.active_tab + len - 1) % len };
        let tab = self.tabs.remove(self.active_tab);
        self.tabs.insert(target, tab);
        self.active_tab = target;
        self.state_changed = true;
    }

    fn restore_document(&mut self, content: &str, tab_index: usize, selection_path: &[usize]) {
        self.tabs = document::parse(content, self.config.general.file_indent);
        self.active_tab = 0;
        let first = self.tabs.remove(0);
        self.selection = first.selection;
        self.tasks = first.tasks;
        self.tabs.insert(0, Tab::new(first.name, self.config.general.file_indent));
        self.switch_tab(tab_index.min(self.tabs.len() - 1));
        self.selection = self.tasks.resolve_path(selection_path).unwrap_or(self.selection);
    }

    fn start_tab_edit(&mut self, added: bool) {
        self.tab_edit_previous = self.active_tab;
        if added {
            let index = self.active_tab + 1;
            self.tabs.insert(index, Tab::new("todo".to_string(), self.config.general.file_indent));
            self.switch_tab(index);
            self.tab_before_edit = None;
            self.text_input = Input::new(String::new());
        } else {
            let name = self.tabs[self.active_tab].name.clone();
            self.tab_before_edit = Some(name.clone());
            self.text_input = Input::new(name);
        }
        self.input_mode = InputMode::EditTab;
    }

    fn finish_tab_edit(&mut self, cancel: bool) -> bool {
        let name = self.text_input.value().trim();
        let confirmed = !cancel && !name.is_empty();
        if !confirmed {
            if let Some(original) = self.tab_before_edit.take() {
                self.tabs[self.active_tab].name = original;
                let tab = self.tabs.remove(self.active_tab);
                self.tabs.insert(self.tab_edit_previous, tab);
                self.active_tab = self.tab_edit_previous;
            } else {
                let added = self.active_tab;
                let previous = if added <= self.tab_edit_previous {
                    self.tab_edit_previous + 1
                } else {
                    self.tab_edit_previous
                };
                self.switch_tab(previous);
                self.tabs.remove(added);
                if added < self.active_tab { self.active_tab -= 1; }
            }
            self.pending_before_selection_path = None;
            self.pending_before_tab = None;
            self.state_changed = false;
        } else {
            self.tabs[self.active_tab].name = name.to_string();
            self.tab_before_edit = None;
            self.state_changed = true;
        }
        self.input_mode = InputMode::Normal;
        confirmed
    }

    pub fn new() -> Tui {
        let config = Config::load().unwrap();

        // Read the todo file
        let content = match fs::read_to_string(&config.general.todo_file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let initial = "# todo\n";
                fs::write(&config.general.todo_file, initial).unwrap();
                initial.to_string()
            }
            Err(e) => panic!("Failed to read todo file: {e}"),
        };
        let mut tabs = document::parse(&content, config.general.file_indent);
        let active = tabs.remove(0);
        let tasks = active.tasks;
        let selection = active.selection;
        tabs.insert(0, Tab::new(active.name, config.general.file_indent));
        let serialized = document::serialize(&document::parse(&content, config.general.file_indent), config.general.file_indent);
        let expected = if content.lines().any(|line| line.starts_with("# ")) {
            content.clone()
        } else {
            format!("# todo\n{content}")
        };
        let meaningful_lines = |s: &str| s.lines().filter(|line| !line.trim().is_empty()).collect::<Vec<_>>().join("\n");
        assert_eq!(meaningful_lines(&expected), meaningful_lines(&serialized));

        Tui {
            config,
            tasks,
            selection,
            scroll_offset: 0,
            tabs,
            active_tab: 0,
            tab_before_edit: None,
            tab_edit_previous: 0,
            text_input: Input::new(String::new()),
            clipboard: None,
            date_picker: None,
            search_query: String::new(),
            search_origin: None,
            search_forward: true,
            input_mode: InputMode::Normal,
            state_changed: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_before_selection_path: None,
            pending_before_tab: None,
            history_content: content.clone(),
            saved_content: content,
        }
    }

    fn copy_selection_to_clipboard(&mut self) {
        if self.tasks.is_root(self.selection) {
            return;
        }
        let mut lines = Vec::new();
        self.tasks.serialize_node(&mut lines, self.tasks.get_node(self.selection), self.config.general.file_indent);
        self.clipboard = Some(lines.join("\n") + "\n");
    }

    fn ensure_task_for_edit(&mut self) {
        if self.tasks.is_root(self.selection) {
            if self.pending_before_selection_path.is_none() {
                self.pending_before_selection_path = Some(Vec::new());
            }
            self.selection = self.tasks.add_top_level();
            self.state_changed = true;
        }
    }

    fn begin_editing(&mut self) {
        self.ensure_task_for_edit();
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
            self.state_changed = true;
        }
        else {
            self.tasks.set_title(self.selection, title);
            self.state_changed = true;
        };
    }

    fn cancel_editing(&mut self) {
        let tab = self.pending_before_tab.take().unwrap_or(self.active_tab);
        let path = self.pending_before_selection_path.take()
            .unwrap_or_else(|| self.tasks.node_to_path(self.selection));
        let content = self.history_content.clone();
        self.restore_document(&content, tab, &path);
        self.state_changed = false;
        self.input_mode = InputMode::Normal;
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

    fn update_date_picker(&mut self, key_event: KeyEvent) -> bool {
        let action = self.config.date_picker_keymap.dispatch(key_event)
            .copied()
            .unwrap_or(Action::NoOp);

        match action {
            Action::Quit => {
                self.close_date_picker();
                return true;
            }
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
        false
    }

    fn save_change(&mut self, previous_selection_path: Vec<usize>, previous_tab: usize) {
        if self.input_mode != InputMode::Normal || !self.state_changed {
            return;
        }

        let before_content = self.history_content.clone();
        let after_content = self.serialize();
        if before_content != after_content {
            let before_selection_path = self.pending_before_selection_path
                .take()
                .unwrap_or(previous_selection_path);
            let before_tab = self.pending_before_tab.take().unwrap_or(previous_tab);
            let after_selection_path = self.tasks.node_to_path(self.selection);
            self.undo_stack.push(HistoryEntry {
                before_content,
                after_content: after_content.clone(),
                before_selection_path,
                after_selection_path,
                before_tab,
                after_tab: self.active_tab,
            });
            self.redo_stack.clear();
            self.history_content = after_content;
            if self.config.general.autosave {
                self.save_to_disk();
            }
        } else {
            self.pending_before_selection_path = None;
            self.pending_before_tab = None;
        }
        self.state_changed = false;
    }

    fn save_to_disk(&mut self) {
        fs::write(&self.config.general.todo_file, &self.history_content).unwrap();
        self.saved_content = self.history_content.clone();
    }

    fn has_unsaved_changes(&self) -> bool {
        let editing_changed = match self.input_mode {
            InputMode::Edit => self.serialize() != self.history_content,
            InputMode::EditTab => self.serialize() != self.history_content
                || self.text_input.value() != self.tabs[self.active_tab].name,
            _ => false,
        };
        !self.config.general.autosave
            && (self.history_content != self.saved_content || editing_changed)
    }

    fn request_quit(&mut self) -> bool {
        if self.has_unsaved_changes() {
            self.input_mode = InputMode::QuitConfirm;
            false
        } else {
            true
        }
    }

    fn search_next(&mut self, from: NodeId, forward: bool) {
        let ids = self.tasks.all_ids();
        if ids.is_empty() || self.search_query.is_empty() {
            self.selection = from;
            return;
        }
        let start = ids.iter().position(|id| *id == from).unwrap_or(0);
        for step in 1..=ids.len() {
            let index = if forward {
                (start + step) % ids.len()
            } else {
                (start + ids.len() - step) % ids.len()
            };
            if self.tasks.get_task(ids[index]).title.contains(&self.search_query) {
                self.selection = ids[index];
                return;
            }
        }
        self.selection = self.search_origin.unwrap_or(from);
    }

    fn update_search(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                self.search_origin = None;
                self.search_query.clear();
            }
            KeyCode::Esc => {
                if let Some(origin) = self.search_origin.take() {
                    self.selection = origin;
                }
                self.search_query.clear();
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let forward = key_event.code == KeyCode::Tab
                    && !key_event.modifiers.contains(KeyModifiers::SHIFT);
                self.search_next(self.selection, forward);
            }
            _ => {
                self.text_input.handle_event(&Event::Key(key_event));
                self.search_query = self.text_input.value().to_string();
                if let Some(origin) = self.search_origin {
                    self.search_next(origin, self.search_forward);
                }
            }
        }
    }

    // Process input. Returns true if the loop should exit.
    fn update(&mut self, key_event: KeyEvent) -> bool {
        if self.input_mode == InputMode::QuitConfirm {
            match key_event.code {
                KeyCode::Char('s' | 'S') => {
                    self.save_to_disk();
                    return true;
                }
                KeyCode::Char('d' | 'D') => return true,
                KeyCode::Esc | KeyCode::Char('c' | 'C') => {
                    self.input_mode = InputMode::Normal;
                }
                _ => {}
            }
            return false;
        }

        if key_event.code == KeyCode::Char('s') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
            let prev_selection_path = self.tasks.node_to_path(self.selection);
            let prev_tab = self.active_tab;
            match self.input_mode {
                InputMode::Edit => self.finish_editing(),
                InputMode::EditTab => { self.finish_tab_edit(false); }
                _ => {}
            }
            self.save_change(prev_selection_path, prev_tab);
            if self.history_content != self.saved_content {
                self.save_to_disk();
            }
            return false;
        }

        if self.input_mode == InputMode::Search {
            self.update_search(key_event);
            return false;
        }
        let prev_selection_path = self.tasks.node_to_path(self.selection);
        let prev_tab = self.active_tab;

        if self.input_mode == InputMode::EditTab {
            let action = self.config.text_keymap.dispatch(key_event)
                .copied()
                .unwrap_or(Action::NoOp);
            let mut quit = false;
            match action {
                Action::AddBelow => {
                    let added = self.tab_before_edit.is_none();
                    if added && self.text_input.value().trim().is_empty() {
                        self.text_input = Input::new("todo".to_string());
                    }
                    if self.finish_tab_edit(false) {
                        self.save_change(prev_selection_path.clone(), prev_tab);
                        self.pending_before_selection_path = Some(self.tasks.node_to_path(self.selection));
                        self.selection = if self.tasks.is_root(self.selection) {
                            self.tasks.add_top_level()
                        } else {
                            self.tasks.add_sibling_below(self.selection)
                        };
                        self.begin_editing();
                        self.state_changed = true;
                    }
                }
                Action::EditDone => {
                    self.finish_tab_edit(false);
                }
                Action::Cancel => { self.finish_tab_edit(true); }
                Action::MoveIn => self.move_tab(true),
                Action::MoveOut => self.move_tab(false),
                Action::Quit => {
                    self.finish_tab_edit(false);
                    quit = true;
                }
                _ => { self.text_input.handle_event(&Event::Key(key_event)); }
            }
            self.save_change(prev_selection_path, prev_tab);
            return quit && self.request_quit();
        }

        // Handle date picker mode separately
        if self.input_mode == InputMode::DatePicker {
            let should_quit = self.update_date_picker(key_event);
            self.save_change(prev_selection_path, prev_tab);
            return should_quit && self.request_quit();
        }

        // Resolve action from the appropriate keymap
        let action = match self.input_mode {
            InputMode::Edit => self.config.text_keymap.dispatch(key_event),
            InputMode::Normal => self.config.normal_keymap.dispatch(key_event),
            InputMode::EditTab => unreachable!(),
            InputMode::DatePicker => unreachable!(),
            InputMode::Search => unreachable!(),
            InputMode::QuitConfirm => unreachable!(),
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
            Action::SelectionFirst => {
                if let Some(id) = self.tasks.all_ids().first() {
                    self.selection = *id;
                }
            },
            Action::SelectionLast => {
                if let Some(id) = self.tasks.all_ids().last() {
                    self.selection = *id;
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
                self.ensure_task_for_edit();
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
            Action::Cancel => {
                if self.input_mode == InputMode::Edit {
                    self.cancel_editing();
                }
            }
            Action::TabPrev => {
                if self.active_tab > 0 { self.switch_tab(self.active_tab - 1); }
            }
            Action::TabNext => {
                self.switch_tab(self.active_tab + 1);
            }
            Action::TabRename => {
                self.pending_before_selection_path = Some(prev_selection_path.clone());
                self.pending_before_tab = Some(prev_tab);
                self.start_tab_edit(false);
            }
            Action::TabAdd => {
                self.pending_before_selection_path = Some(prev_selection_path.clone());
                self.pending_before_tab = Some(prev_tab);
                self.start_tab_edit(true);
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
                if self.tasks.is_root(self.selection) && self.tasks.all_ids().is_empty() {
                    if self.tabs.len() == 1 {
                        if self.config.general.autosave {
                            fs::remove_file(&self.config.general.todo_file).unwrap();
                            return true;
                        }
                        return self.request_quit();
                    }
                    let deleted = self.active_tab;
                    let next = if deleted + 1 < self.tabs.len() { deleted + 1 } else { deleted - 1 };
                    self.switch_tab(next);
                    self.tabs.remove(deleted);
                    if deleted < self.active_tab { self.active_tab -= 1; }
                    self.state_changed = true;
                } else {
                    self.copy_selection_to_clipboard();
                    self.selection = self.tasks.remove(self.selection);
                    self.state_changed = true;
                }
            }
            Action::Copy => {
                self.copy_selection_to_clipboard();
            }
            Action::PasteBelow => {
                if let Some(ref content) = self.clipboard {
                    self.selection = self.tasks.paste_branch_below(self.selection, content, self.config.general.file_indent);
                    self.state_changed = true;
                }
            }
            Action::PasteAbove => {
                if let Some(ref content) = self.clipboard {
                    self.selection = self.tasks.paste_branch_above(self.selection, content, self.config.general.file_indent);
                    self.state_changed = true;
                }
            }
            Action::Undo => {
                if let Some(entry) = self.undo_stack.pop() {
                    self.restore_document(&entry.before_content, entry.before_tab, &entry.before_selection_path);
                    self.history_content = entry.before_content.clone();
                    if self.config.general.autosave { self.save_to_disk(); }
                    self.redo_stack.push(entry);
                }
            }
            Action::Redo => {
                if let Some(entry) = self.redo_stack.pop() {
                    self.restore_document(&entry.after_content, entry.after_tab, &entry.after_selection_path);
                    self.history_content = entry.after_content.clone();
                    if self.config.general.autosave { self.save_to_disk(); }
                    self.undo_stack.push(entry);
                }
            }
            Action::SearchForward | Action::SearchBackward => {
                self.search_origin = Some(self.selection);
                self.search_forward = action == Action::SearchForward;
                self.search_query.clear();
                self.text_input = Input::new(String::new());
                self.input_mode = InputMode::Search;
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

        self.save_change(prev_selection_path, prev_tab);

        should_quit && self.request_quit()
    }

    fn cursor_position(&self) -> (u16, u16) {
        let all_ids = self.tasks.all_ids();
        let selected_idx = all_ids.iter().position(|&id| id == self.selection).unwrap_or(0);
        // Row: 1 (title bar / top border) + selected task index in the viewport
        let row = 1 + selected_idx.saturating_sub(self.scroll_offset) as u16;
        // Column: 1 (left border) + indent prefix + 1 (marker) + 1 (space after marker)
        let node = self.tasks.get_node(self.selection);
        let indent = node.ancestors().count() - 1;
        let col: u16 = (1 + indent * self.config.general.display_indent + 1 + 1
            + self.text_input.cursor()) as u16;
        (col, row)
    }

    fn tab_cursor_position(&self) -> (u16, u16) {
        let preceding: usize = self.tabs.iter().take(self.active_tab)
            .map(|tab| tab.name.chars().count() + 2).sum();
        ((2 + preceding + self.text_input.cursor()) as u16, 0)
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let inner_width = area.width.saturating_sub(2) as usize; // subtract borders
        let lines = self.tasks.display(self.config.general.display_indent, self.selection, inner_width, &self.search_query, &self.config.colors);
        let visible_height = area.height.saturating_sub(2) as usize;
        if visible_height == 0 || lines.len() <= visible_height {
            self.scroll_offset = 0;
        } else {
            let selected_idx = self.tasks.all_ids().iter().position(|&id| id == self.selection).unwrap_or(0);
            let scrolloff = usize::from(visible_height >= 3);
            if selected_idx < self.scroll_offset + scrolloff {
                self.scroll_offset = selected_idx.saturating_sub(scrolloff);
            } else if selected_idx >= self.scroll_offset + visible_height - scrolloff {
                self.scroll_offset = selected_idx + scrolloff + 1 - visible_height;
            }
            self.scroll_offset = self.scroll_offset.min(lines.len() + scrolloff - visible_height);
        }
        let text = Text::from(lines);
        let unsaved = self.has_unsaved_changes();
        let titles: Vec<Span> = self.tabs.iter().enumerate().map(|(index, tab)| {
            let name = if index == self.active_tab && self.input_mode == InputMode::EditTab {
                self.text_input.value()
            } else {
                &tab.name
            };
            let style = if index == self.active_tab && self.input_mode == InputMode::EditTab {
                Style::default().fg(self.config.colors.active_tab_bg).bg(self.config.colors.active_tab_fg).bold()
            } else if index == self.active_tab {
                Style::default().fg(self.config.colors.active_tab_fg).bg(self.config.colors.active_tab_bg).bold()
            } else {
                Style::default().fg(self.config.colors.inactive_tab)
            };
            Span::styled(format!(" {} ", name), style)
        }).collect();
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.config.colors.border))
            .title(Line::from(titles));
        if unsaved {
            block = block.title_top(Line::from(Span::styled(" * ", Style::default().fg(self.config.colors.upcoming))).right_aligned());
        }
        if self.input_mode == InputMode::Search {
            let prefix = if self.search_forward { '/' } else { '?' };
            block = block.title_bottom(format!("{}{}", prefix, self.search_query));
        }
        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(self.config.colors.text).bg(self.config.colors.background))
            .block(block)
            .scroll((self.scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);

        // Draw date picker overlay if active
        if let Some(ref picker) = self.date_picker {
            self.draw_date_picker(frame, area, picker);
        }
        if self.input_mode == InputMode::QuitConfirm {
            let width = 39.min(area.width);
            let height = 4.min(area.height);
            let popup_area = Rect::new(
                area.x + (area.width - width) / 2,
                area.y + (area.height - height) / 2,
                width,
                height,
            );
            let dialog = Paragraph::new("Save changes before quitting?\n[S] Save  [D] Discard  [Esc] Cancel")
                .style(Style::default().fg(self.config.colors.text).bg(self.config.colors.background))
                .alignment(Alignment::Center)
                .block(Block::bordered().border_style(Style::default().fg(self.config.colors.border)));
            frame.render_widget(Clear, popup_area);
            frame.render_widget(dialog, popup_area);
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
        events.add(picker.cursor_date, Style::default().bg(self.config.colors.calendar_selection_bg).bold());

        let base_style = Style::default().fg(self.config.colors.text).bg(self.config.colors.background);
        let calendar = Monthly::new(picker.display_date, events)
            .default_style(base_style)
            .show_month_header(Style::default().fg(self.config.colors.text).bold())
            .show_weekdays_header(Style::default().fg(self.config.colors.muted))
            .block(
                Block::bordered()
                    .style(base_style)
                    .border_style(Style::default().fg(self.config.colors.border))
                    .title(Span::styled(title, Style::default().fg(self.config.colors.text).bold())),
            );

        frame.render_widget(Clear, popup_area);
        frame.render_widget(Block::default().style(base_style), popup_area);
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

                if matches!(self.input_mode, InputMode::Edit | InputMode::EditTab | InputMode::Search) {
                    let (col, row) = if self.input_mode == InputMode::Search {
                        (2 + self.text_input.cursor() as u16, terminal.size()?.height.saturating_sub(1))
                    } else if self.input_mode == InputMode::EditTab {
                        self.tab_cursor_position()
                    } else {
                        self.cursor_position()
                    };
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
