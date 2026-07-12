use ratatui::{prelude::*, text::Line};
use ego_tree::{NodeId, NodeMut, NodeRef, Tree};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Task {
    pub title: String,
    pub completed: bool,
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
        Some(Task {
            title,
            completed,
        })
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
    pub fn new() -> (TaskTree, NodeId) {
        let tasks = Tree::new(Task::default());
        let selection = tasks.root().id();
        (TaskTree { tasks }, selection)
    }

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
                while parent_stack.last().map_or(false, |(_, ind)| *ind >= indent) {
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
        let line = format!("{}- {} {}", prefix, checkbox, node.value().title);
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

    pub fn display_node(&self, lines: &mut Vec<Line>, node: NodeRef<'_, Task>, indent_width: usize, selected_id: NodeId) {
        let indent = node.ancestors().count() - 1;
        let task = node.value();
        let is_first_actionable = self.is_first_actionable(node.id());
        let is_selected = node.id() == selected_id;
        let is_ancestor_of_selected = node.descendants().any(|n| n.id() == selected_id);
        let is_sibling_of_selected = node.ancestors().any(|n| n.id() == self.tasks.get(selected_id).unwrap().parent().unwrap().id());
        let is_descendant_of_selected = node.ancestors().any(|n| n.id() == selected_id);

        let prefix = "\u{00A0}".repeat(indent * indent_width);
        let marker = if node.value().completed { "◉" } else { "◯" };
        let title = &node.value().title;

        let style = Style::default();
        // Text color
        let style = if task.completed {
            style.fg(Color::Rgb(56, 56, 64))
        } else if node.first_child().is_some() && is_first_actionable {
            style
        } else if is_first_actionable {
            style.green().bold()
        } else {
            style.fg(Color::Rgb(112, 112, 128))
        };
        // Background color
        let style = if is_selected || is_descendant_of_selected {
            style.bg(Color::Rgb(56, 56, 64))
        } else if !is_sibling_of_selected && !is_ancestor_of_selected {
            style.dim()
        } else {
            style
        };

        let line = Line::from(format!("{}{} {}", prefix, marker, title)).patch_style(style);
        lines.push(line);

        for child in node.children() {
            self.display_node(lines, child, indent_width, selected_id);
        }
    }

    pub fn display(&self, indent_width: usize, selected_id: NodeId) -> Vec<Line<'_>> {
        let mut lines: Vec<Line> = Vec::new();
        for child in self.tasks.root().children() {
            self.display_node(&mut lines, child, indent_width, selected_id);
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
        let node = self.get_node(id);
        if let Some(child) = node.first_child() { Some(child.id()) }
        else if let Some(sibling) = node.next_sibling() { Some(sibling.id()) }
        // TODO Keep walking up ancestors
        else { node.parent().unwrap().next_sibling().map(|pibling| pibling.id()) }
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

    fn is_root(&self, id: NodeId) -> bool {
        id == self.tasks.root().id()
    }

    fn is_top_level(&self, id: NodeId) -> bool {
        self.get_node(id).parent().unwrap().id() == self.tasks.root().id()
    }

    pub fn has_children(&self, id: NodeId) -> bool {
        self.get_node(id).has_children()
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
        Some(id)
    }

    pub fn move_in(&mut self, id: NodeId) -> Option<NodeId> {
        if self.is_root(id) { return None; }
        let prev_sibling = self.get_node(id).prev_sibling();
        let target_parent = prev_sibling.map(|s| s.id())
            .or_else(|| self.get_parent_non_root(id));
        let target_parent = match target_parent {
            Some(p) => p,
            None => return None, // top-level with no previous sibling
        };
        // Detach the node from its current parent
        self.get_node_mut(id).detach();
        // Append as last child of target parent
        self.get_node_mut(target_parent).append_id(id);
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
        self.tasks.root_mut().prepend(task).id()
    }

    pub fn add_sibling_below(&mut self, id: NodeId) -> NodeId {
        // Create sibling node
        let task = Task::default();
        let parent = self.get_node(id).parent().unwrap().id();
        let added_id = self.get_node_mut(parent).append(task).id();

        // Move it to under id
        while let Some(above) = self.get_node(added_id).prev_sibling() {
            if above.id() == id {
                break;
            }
            self.swap_siblings(added_id, above.id());
        }
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
        self.get_node_mut(id).prepend(task).id()
    }

    pub fn remove(&mut self, id: NodeId) -> NodeId {
        if self.is_root(id) { return id; }
        let node = self.get_node(id);
        let next_selected = node.prev_sibling().map(|n| n.id()).unwrap_or(
            node.next_sibling().map(|n| n.id()).unwrap_or(
                node.parent().unwrap().id()
            )
        );
        // Detach all descendants, then detach the node itself
        let descendant_ids: Vec<NodeId> = node.descendants().skip(1).map(|n| n.id()).collect();
        for desc_id in descendant_ids.iter().rev() {
            self.get_node_mut(*desc_id).detach();
        }
        self.get_node_mut(id).detach();
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
        let node = self.get_node(id);
        let ancestors: Vec<NodeId> = node.ancestors().map(|node| node.id()).collect();
        for id in ancestors {
            let all_children_completed = self.get_node(id)
                .children()
                .all(|child| child.value().completed);
            self.get_node_mut(id).value().completed = all_children_completed;
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
}
