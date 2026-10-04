use std::str::FromStr;

use ratatui::{prelude::*, text::Line};
use ego_tree::{NodeId, NodeMut, NodeRef, Tree};
use crate::config::Colors;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Task {
    pub title: String,
    pub completed: bool,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
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
        let rest = trimmed[close_bracket + 1..].trim().to_string();
        if rest.is_empty() {
            return None;
        }

        // Extract trailing key:value metadata (start:DATE, due:DATE)
        // Scan tokens from right to left; stop once a non-metadata token is hit
        let mut start_date = None;
        let mut due_date = None;
        let tokens: Vec<&str> = rest.split(' ').collect();
        let mut metadata_count = 0;
        for &token in tokens.iter().rev() {
            if let Some(val) = token.strip_prefix("start:") {
                if Self::is_valid_date(val) {
                    start_date = Some(val.to_string());
                    metadata_count += 1;
                    continue;
                }
            }
            if let Some(val) = token.strip_prefix("due:") {
                if Self::is_valid_date(val) {
                    due_date = Some(val.to_string());
                    metadata_count += 1;
                    continue;
                }
            }
            // Non-metadata token — stop scanning
            break;
        }

        // Title is everything before the trailing metadata tokens
        let title_end = tokens.len() - metadata_count;
        let title = tokens[..title_end].join(" ");

        Some(Task {
            title,
            completed,
            start_date,
            due_date,
        })
    }

    fn is_valid_date(s: &str) -> bool {
        if s.len() != 10 { return false; }
        let bytes = s.as_bytes();
        matches!(bytes, [b'0'..=b'9', b'0'..=b'9', b'0'..=b'9', b'0'..=b'9', b'-', b'0'..=b'9', b'0'..=b'9', b'-', b'0'..=b'9', b'0'..=b'9'])
    }

    fn is_date_past_or_today(s: &str) -> Option<bool> {
        let today = time::OffsetDateTime::now_local()
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
            .date();
        let bytes = s.as_bytes();
        let year = i32::from_str(std::str::from_utf8(&bytes[0..4]).ok()?).ok()?;
        let month_u8 = u8::from_str(std::str::from_utf8(&bytes[5..7]).ok()?).ok()?;
        let day = u8::from_str(std::str::from_utf8(&bytes[8..10]).ok()?).ok()?;
        let date = time::Date::from_calendar_date(year, time::Month::try_from(month_u8).ok()?, day).ok()?;
        Some(date <= today)
    }

    fn is_date_future(s: &str) -> Option<bool> {
        Self::is_date_past_or_today(s).map(|v| !v)
    }
}

#[derive(Debug)]
pub struct TaskTree {
    tasks: Tree<Task>,
}

impl PartialEq for TaskTree {
    fn eq(&self, other: &Self) -> bool {
        self.tasks == other.tasks
    }
}

impl TaskTree {
    pub fn from_string(string: &str, indent_width: usize) -> (TaskTree, NodeId) {
        let mut tasks = Tree::new(Task::default());
        let root_id = tasks.root().id();

        // Stack of (parent_node_id, indent_level) to track hierarchy
        let mut parent_stack: Vec<(NodeId, i32)> = vec![(root_id, -1)];

        for line in string.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let indent = line.chars().take_while(|c| *c == ' ').count();
            assert!(indent % indent_width == 0);
            let indent = (indent / indent_width) as i32;

            if let Some(task) = Task::from_str(trimmed) {
                // Pop the stack until we find a parent with a smaller indent level
                while parent_stack.last().is_some_and(|(_, ind)| *ind >= indent) {
                    parent_stack.pop();
                }

                let parent_id = parent_stack.last().unwrap().0;
                let mut parent_node = tasks.get_mut(parent_id).unwrap();
                let child_node = parent_node.append(task);
                let child_id = child_node.id();

                parent_stack.push((child_id, indent));
            }
        }

        let selection = tasks.root().first_child().map(|first| first.id()).unwrap_or(root_id);
        (TaskTree { tasks }, selection)
    }

    pub fn serialize_node(&self, lines: &mut Vec<String>, node: NodeRef<'_, Task>, indent_width: usize) {
        let indent = node.ancestors().count() - 1;
        let prefix = " ".repeat(indent * indent_width);
        let checkbox = if node.value().completed { "[x]" } else { "[ ]" };
        let task = node.value();
        let mut line = format!("{}- {} {}", prefix, checkbox, task.title);
        if let Some(ref start) = task.start_date {
            line.push_str(&format!(" start:{}", start));
        }
        if let Some(ref due) = task.due_date {
            line.push_str(&format!(" due:{}", due));
        }
        lines.push(line);

        for child in node.children() {
            self.serialize_node(lines, child, indent_width);
        }
    }

    pub fn to_string(&self, indent_width: usize) -> String {
        let mut lines: Vec<String> = Vec::new();

        for child in self.tasks.root().children() {
            self.serialize_node(&mut lines, child, indent_width);
        }

        lines.join("\n") + "\n"
    }

    fn insert_branch_at_sibling(&mut self, target_id: NodeId, branch_content: &str, after: bool, indent_width: usize) -> NodeId {
        let (branch_tree, _) = Self::from_string(branch_content, indent_width);
        if self.is_root(target_id) {
            let mut first_pasted_id = target_id;
            for branch_child in branch_tree.tasks.root().children() {
                let new_id = self.clone_subtree(branch_child, target_id, false);
                if first_pasted_id == target_id {
                    first_pasted_id = new_id;
                }
            }
            return first_pasted_id;
        }
        let parent_id = self.get_node(target_id).parent().unwrap().id();

        let mut first_pasted_id = target_id;
        let mut branch_children: Vec<_> = branch_tree.tasks.root().children().collect();
        if after {
            branch_children.reverse();
        }

        for branch_child in branch_children {
            let new_id = self.clone_subtree(branch_child, parent_id, true);
            if after || first_pasted_id == target_id {
                first_pasted_id = new_id;
            }

            // Position: if `after`, move after target; otherwise move before target
            while let Some(next) = self.get_node(new_id).next_sibling() {
                if next.id() == target_id {
                    if after {
                        self.swap_siblings(new_id, next.id());
                    }
                    break;
                }
                self.swap_siblings(new_id, next.id());
            }
        }

        first_pasted_id
    }

    fn clone_subtree(&mut self, node: NodeRef<'_, Task>, parent_id: NodeId, prepend: bool) -> NodeId {
        let task = (*node.value()).clone();
        let new_id = if prepend {
            self.get_node_mut(parent_id).prepend(task).id()
        } else {
            self.get_node_mut(parent_id).append(task).id()
        };

        for child in node.children() {
            self.clone_subtree(child, new_id, false);
        }

        new_id
    }

    pub fn paste_branch_below(&mut self, id: NodeId, content: &str, indent_width: usize) -> NodeId {
        self.insert_branch_at_sibling(id, content, true, indent_width)
    }

    pub fn paste_branch_above(&mut self, id: NodeId, content: &str, indent_width: usize) -> NodeId {
        self.insert_branch_at_sibling(id, content, false, indent_width)
    }

    fn title_spans(spans: &mut Vec<Span<'static>>, prefix: &str, title: &str, style: Style, query: &str, colors: &Colors) {
        spans.push(Span::styled(prefix.to_string(), style));
        if query.is_empty() {
            spans.push(Span::styled(title.to_string(), style));
            return;
        }
        let mut end = 0;
        for (index, matched) in title.match_indices(query) {
            spans.push(Span::styled(title[end..index].to_string(), style));
            spans.push(Span::styled(matched.to_string(), style.fg(colors.search_fg).bg(colors.search_bg)));
            end = index + matched.len();
        }
        spans.push(Span::styled(title[end..].to_string(), style));
    }

    pub fn display_node(&self, lines: &mut Vec<Line>, node: NodeRef<'_, Task>, indent_width: usize, selected_id: NodeId, width: usize, query: &str, colors: &Colors) {
        let indent = node.ancestors().count() - 1;
        let task = node.value();
        let is_first_actionable = self.is_first_actionable(node.id());
        let is_selected = node.id() == selected_id;
        let is_ancestor_of_selected = node.descendants().skip(1).any(|n| n.id() == selected_id);
        let has_same_ancestry_as_selected = node.ancestors().any(|n| n.id() == self.tasks.get(selected_id).unwrap().parent().unwrap().id());
        let is_descendant_of_selected = node.ancestors().any(|n| n.id() == selected_id);
        let start_future = task.start_date.as_ref().and_then(|s| Task::is_date_future(s)).unwrap_or(false);
        let any_ancestor_start_future = node.ancestors().any(|n| {
            n.value().start_date.as_ref().and_then(|s| Task::is_date_future(s)).unwrap_or(false)
        });
        let due_past_or_today = task.due_date.as_ref().and_then(|s| Task::is_date_past_or_today(s)).unwrap_or(false);

        let style = Style::default().fg(colors.text);
        // Text color
        let style = if task.completed {
            // Completed
            style.fg(colors.completed)
        } else if due_past_or_today {
            // (Over)due
            style.fg(colors.overdue).bold()
        } else if start_future {
            // Not starting yet
            style.fg(colors.upcoming)
        } else if any_ancestor_start_future {
            // Not starting yet (descendant)
            style.fg(colors.muted)
        } else if node.first_child().is_some() && is_first_actionable {
            // Parent
            style
        } else if is_first_actionable {
            // First actionable
            style.fg(colors.actionable)
        } else if is_ancestor_of_selected {
            // Ancestor
            style
        } else {
            // Later
            style.fg(colors.muted)
        };
        let style = if is_first_actionable { style.bold() } else { style };
        let dim = if !is_ancestor_of_selected && !has_same_ancestry_as_selected {
            Style::new().add_modifier(Modifier::DIM)
        } else {
            Style::new()
        };
        // Background color
        let background = if is_selected {
            Style::new().bg(colors.selection_bg)
        } else if is_descendant_of_selected {
            Style::new().bg(colors.descendant_bg)
        } else {
            Style::new()
        };
        let style = style.patch(background).patch(dim);

        // Build individual date spans with their own styles
        let mut date_spans: Vec<Span> = vec![];
        if let Some(ref start) = task.start_date {
            let start_text = format!("start: {}", start);
            let s = if !start_future {
                Style::default().fg(colors.completed)
            } else {
                Style::default().fg(colors.muted)
            }.patch(background).patch(dim);
            date_spans.push(Span::styled(start_text, s));
        }
        if let Some(ref due) = task.due_date {
            let due_text = format!("due: {}", due);
            let s = if due_past_or_today {
                Style::default().fg(colors.overdue).bold()
            } else if start_future {
                Style::default().fg(colors.completed)
            } else {
                Style::default().fg(colors.upcoming)
            }.patch(background).patch(dim);
            // Add a separator if both dates are present
            if task.start_date.is_some() {
                date_spans.push(Span::styled(" ", background));
            }
            date_spans.push(Span::styled(due_text, s));
        }

        // Calculate total date width from spans
        let date_width: usize = date_spans.iter().map(|sp| sp.width()).sum();

        let prefix = "\u{00A0}".repeat(indent * indent_width);
        let marker = if node.value().completed { "◉" } else { "◯" };
        let title = &node.value().title;
        let title_prefix = format!("{}{} ", prefix, marker);
        let title_content = format!("{}{}", title_prefix, title);
        let title_width = title_content.chars().count();

        // If there are dates and enough room, right-justify them; otherwise inline after title
        let mut spans: Vec<Span> = vec![];
        let remaining = width.saturating_sub(title_width);

        if !date_spans.is_empty() && remaining > date_width {
            // Right-justified dates with padding between title and dates
            let gap = remaining - date_width;
            let gap_fill = "\u{00A0}".repeat(gap);
            Self::title_spans(&mut spans, &title_prefix, title, style, query, colors);
            spans.push(Span::styled(gap_fill, style));
            for ds in date_spans {
                spans.push(ds);
            }
        } else {
            // No dates or not enough room — just the title
            Self::title_spans(&mut spans, &title_prefix, title, style, query, colors);
            let fill_remaining = width.saturating_sub(title_width);
            if fill_remaining > 0 {
                let fill = "\u{00A0}".repeat(fill_remaining);
                spans.push(Span::styled(fill, style));
            }
        }

        lines.push(Line::from(spans));

        for child in node.children() {
            self.display_node(lines, child, indent_width, selected_id, width, query, colors);
        }
    }

    pub fn display(&self, indent_width: usize, selected_id: NodeId, width: usize, query: &str, colors: &Colors) -> Vec<Line<'_>> {
        let mut lines: Vec<Line> = Vec::new();
        for child in self.tasks.root().children() {
            self.display_node(&mut lines, child, indent_width, selected_id, width, query, colors);
        }
        lines
    }

    pub fn all_ids(&self) -> Vec<NodeId> {
        self.tasks.root().descendants().skip(1).map(|node| node.id()).collect()
    }

    pub fn get_node(&self, id: NodeId) -> NodeRef<'_, Task> {
        self.tasks.get(id).unwrap()
    }

    fn get_node_mut(&mut self, id: NodeId) -> NodeMut<'_, Task> {
        self.tasks.get_mut(id).unwrap()
    }

    pub fn get_task(&self, id: NodeId) -> &Task {
        self.get_node(id).value()
    }

    pub fn set_task(&mut self, id: NodeId, task: Task) {
        *self.get_node_mut(id).value() = task;
    }

    pub fn get_parent_non_root(&self, id: NodeId) -> Option<NodeId> {
        if self.is_top_level(id) { None }
        else { Some(self.get_node(id).parent().unwrap().id()) }
    }

    pub fn get_first_child(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).first_child().map(|child| child.id())
    }

    pub fn get_prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).prev_sibling().map(|sib| sib.id())
    }

    pub fn get_next_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).next_sibling().map(|sib| sib.id())
    }

    pub fn get_above(&self, id: NodeId) -> Option<NodeId> {
        let node = self.get_node(id);
        if let Some(sibling) = node.prev_sibling() {
            let descendants: Vec<NodeId> = sibling.descendants().map(|n| n.id()).collect();
            if let Some(last_descendant) = descendants.last() { Some(*last_descendant) }
            else { Some(sibling.id()) }
        }
        else { self.get_parent_non_root(id) }
    }

    pub fn get_below(&self, id: NodeId) -> Option<NodeId> {
        let mut node = self.get_node(id);
        if let Some(child) = node.first_child() { Some(child.id()) }
        else {
            loop {
                if let Some(sibling) = node.next_sibling() { return Some(sibling.id()); }
                node = node.parent()?;
            }
        }
    }

    pub fn switch_with_prev_sibling(&mut self, id: NodeId) -> Option<NodeId> {
        let prev_id = self.get_node(id).prev_sibling().map(|sib| sib.id());
        match prev_id {
            Some(prev_id) => {
                self.swap_siblings(id, prev_id);
                Some(id)
            }
            None => None,
        }
    }

    pub fn switch_with_next_sibling(&mut self, id: NodeId) -> Option<NodeId> {
        let next_id = self.get_node(id).next_sibling().map(|sib| sib.id());
        match next_id {
            Some(next_id) => {
                self.swap_siblings(id, next_id);
                Some(id)
            }
            None => None,
        }
    }

    fn swap_siblings(&mut self, id1: NodeId, id2: NodeId) {
        let parent_id = self.get_node(id1).parent().unwrap().id();
        // Collect sibling chain to rebuild order
        let siblings: Vec<NodeId> = self.get_node(parent_id)
            .children()
            .map(|sib| sib.id())
            .collect();
        let mut new_order = Vec::new();
        for &sid in &siblings {
            if sid == id1 {
                new_order.push(id2);
            } else if sid == id2 {
                new_order.push(id1);
            } else {
                new_order.push(sid);
            }
        }
        // Reattach in new order
        for &sid in &new_order {
            let mut node = self.get_node_mut(sid);
            node.detach();
            self.get_node_mut(parent_id).append_id(sid);
        }
    }

    pub fn is_root(&self, id: NodeId) -> bool {
        id == self.tasks.root().id()
    }

    fn is_top_level(&self, id: NodeId) -> bool {
        self.get_node(id).parent().unwrap().id() == self.tasks.root().id()
    }

    pub fn move_out(&mut self, id: NodeId) -> Option<NodeId> {
        if self.is_root(id) || self.is_top_level(id) { return None; }
        let parent_id = self.get_node(id).parent().unwrap().id();
        // Detach the node from its current parent
        self.get_node_mut(id).detach();
        // Append it as a child of the grandparent (i.e. sibling of old parent)
        let grandparent_id = self.get_node(parent_id).parent().unwrap().id();
        self.get_node_mut(grandparent_id).append_id(id);
        // Move it to after the old parent
        while let Some(prev) = self.get_node(id).prev_sibling() {
            if prev.id() == parent_id { break; }
            self.swap_siblings(id, prev.id());
        }
        self.update_ancestors_completion(parent_id);
        Some(id)
    }

    pub fn move_in(&mut self, id: NodeId) -> Option<NodeId> {
        if self.is_root(id) { return None; }
        let target_parent = self.get_node(id).prev_sibling()?.id();
        // Detach the node from its current parent
        self.get_node_mut(id).detach();
        // Append as last child of target parent
        self.get_node_mut(target_parent).append_id(id);
        self.update_ancestors_completion(id);
        Some(id)
    }

    fn is_first_actionable(&self, id: NodeId) -> bool {
        // All top level tasks are actionable
        let Some(parent) = self.get_parent_non_root(id) else {
            return true;
        };

        // If parent is not first actionable, child isn't either
        if !self.is_first_actionable(parent) {
            return false;
        }

        // If id is done, it's not actionable
        let node = self.get_node(id);
        if node.value().completed { return false; }

        // If any previous sibling not done, then id is not first actionable
        for sib in node.parent().unwrap().children() {
            if sib.id() == id { break; }
            if !sib.value().completed { return false; }
        }
        true
    }

    pub fn add_top_level(&mut self) -> NodeId {
        let task = Task::default();
        let id = self.tasks.root_mut().prepend(task).id();
        self.update_ancestors_completion(id);
        id
    }

    pub fn add_sibling_below(&mut self, id: NodeId) -> NodeId {
        // Create sibling node
        let task = Task::default();
        let parent = self.get_node(id).parent().unwrap_or(self.tasks.root()).id();
        let added_id = self.get_node_mut(parent).append(task).id();

        // Move it to under id
        while let Some(above) = self.get_node(added_id).prev_sibling() {
            if above.id() == id {
                break;
            }
            self.swap_siblings(added_id, above.id());
        }
        self.update_ancestors_completion(added_id);
        added_id
    }

    pub fn add_sibling_above(&mut self, id: NodeId) -> NodeId {
        let added_id = self.add_sibling_below(id);
        if self.get_node(added_id).prev_sibling().is_some() {
            let prev_id = self.get_node(added_id).prev_sibling().unwrap().id();
            self.swap_siblings(added_id, prev_id);
        }
        added_id
    }

    pub fn add_child(&mut self, id: NodeId) -> NodeId {
        let task = Task::default();
        let added_id = self.get_node_mut(id).prepend(task).id();
        self.update_ancestors_completion(added_id);
        added_id
    }

    pub fn remove(&mut self, id: NodeId) -> NodeId {
        if self.is_root(id) { return id; }
        let node = self.get_node(id);
        let next_selected = node.next_sibling().map(|n| n.id()).unwrap_or(
            node.prev_sibling().map(|n| n.id()).unwrap_or(
                node.parent().unwrap().id()
            )
        );
        // Detach all descendants, then detach the node itself
        let descendant_ids: Vec<NodeId> = node.descendants().skip(1).map(|n| n.id()).collect();
        for desc_id in descendant_ids.iter().rev() {
            self.get_node_mut(*desc_id).detach();
        }
        self.get_node_mut(id).detach();
        self.update_ancestors_completion(next_selected);
        next_selected
    }

    fn set_descendants_completion(&mut self, id: NodeId) {
        let node = self.get_node(id);
        let completed = node.value().completed;
        let descendants: Vec<NodeId> = node.descendants().skip(1).map(|node| node.id()).collect();
        for id in descendants {
            self.get_node_mut(id).value().completed = completed;
        }
    }

    fn update_ancestors_completion(&mut self, id: NodeId) {
        let ancestors: Vec<NodeId> = [id].into_iter().chain(self.get_node(id).ancestors().map(|node| node.id())).collect();
        for id in ancestors {
            if self.get_node(id).has_children() {
                let all_children_completed = self.get_node(id)
                    .children()
                    .all(|child| child.value().completed);
                self.get_node_mut(id).value().completed = all_children_completed;
            }
        }
    }

    pub fn toggle_completed(&mut self, id: NodeId) {
        if self.is_root(id) { return; }
        // Toggle id
        let mut node = self.get_node_mut(id);
        node.value().completed = !node.value().completed;
        // Update descendants
        self.set_descendants_completion(id);
        // Update ancestors
        self.update_ancestors_completion(id);
    }

    pub fn set_title(&mut self, id: NodeId, title: String) {
        if self.is_root(id) { return; }
        self.get_node_mut(id).value().title = title;
    }

    // Convert a NodeId to a path of child indices (e.g. [2, 0, 1] = 3rd top-level -> 1st child -> 2nd child)
    pub fn node_to_path(&self, id: NodeId) -> Vec<usize> {
        let node = self.get_node(id);
        let mut path = Vec::new();
        let mut current = node;
        while let Some(parent) = current.parent() {
            // Find the index of current among parent's children
            let idx = parent.children().position(|c| c.id() == current.id());
            if let Some(i) = idx {
                path.push(i);
            }
            current = parent;
        }
        path.reverse();
        path
    }

    // Resolve a path of child indices back to a NodeId. Returns None if path is invalid.
    pub fn resolve_path(&self, path: &[usize]) -> Option<NodeId> {
        let mut current = self.tasks.root();
        for &idx in path {
            let child = current.children().nth(idx)?;
            current = child;
        }
        Some(current.id())
    }
}
