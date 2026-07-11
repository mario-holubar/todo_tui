use ratatui::{prelude::*, text::Line};
use slab_tree::{NodeId, NodeMut, NodeRef, RemoveBehavior::DropChildren, Tree};

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
        let mut tasks = Tree::new();
        tasks.set_root(Task::default());
        let selection = tasks.root_id().unwrap();
        (TaskTree { tasks }, selection)
    }

    pub fn from_string(string: &str, indent_width: usize) -> (TaskTree, NodeId) {
        let mut tasks = Tree::new();
        tasks.set_root(Task::default());
        let root_id = tasks.root_id().unwrap();

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
                let child_id = child_node.node_id();

                parent_stack.push((child_id, indent));
            }
        }

        let selection = tasks.root().unwrap().first_child().map(|first| first.node_id()).unwrap_or(root_id);
        (TaskTree { tasks }, selection)
    }

    pub fn serialize_node(&self, lines: &mut Vec<String>, node: slab_tree::node::NodeRef<'_, Task>, indent_width: usize) {
        let indent = node.ancestors().count() - 1;
        let prefix = " ".repeat(indent * indent_width);
        let checkbox = if node.data().completed { "[x]" } else { "[ ]" };
        let line = format!("{}- {} {}", prefix, checkbox, node.data().title);
        lines.push(line);

        for child in node.children() {
            self.serialize_node(lines, child, indent_width);
        }
    }

    pub fn to_string(&self, indent_width: usize) -> String {
        let root_id = self.tasks.root_id().unwrap();
        let mut lines: Vec<String> = Vec::new();

        if let Some(root) = self.tasks.get(root_id) {
            for child in root.children() {
                self.serialize_node(&mut lines, child, indent_width);
            }
        }

        lines.join("\n") + "\n"
    }

    pub fn display_node(&self, lines: &mut Vec<Line>, node: slab_tree::node::NodeRef<'_, Task>, indent_width: usize) {
        let indent = node.ancestors().count() - 1;
        let task = node.data();
        let is_first_actionable = self.is_first_actionable(node.node_id());

        let prefix = "\u{00A0}".repeat(indent * indent_width);
        let marker = if node.data().completed { "◉" } else { "◯" };
        let title = &node.data().title;

        let mut style = Style::default();
        if task.completed {
            style = style.dark_gray().dim();
        } else if node.first_child().is_some() && is_first_actionable {
            // default style, no change needed
        } else if is_first_actionable {
            style = style.green().bold();
        } else {
            style = style.dim();
        }

        let line = Line::from(format!("{}{} {}", prefix, marker, title)).patch_style(style);
        lines.push(line);

        for child in node.children() {
            self.display_node(lines, child, indent_width);
        }
    }

    pub fn display(&self, indent_width: usize) -> Vec<Line<'_>> {
        let root_id = self.tasks.root_id().unwrap();
        let mut lines: Vec<Line> = Vec::new();
        if let Some(root) = self.tasks.get(root_id) {
            for child in root.children() {
                self.display_node(&mut lines, child, indent_width);
            }
        }
        lines
    }

    pub fn all_ids(&self) -> Vec<NodeId> {
        self.tasks.root().unwrap().traverse_pre_order().skip(1).map(|node| node.node_id()).collect()
    }

    fn get_node(&self, id: NodeId) -> NodeRef<'_, Task> {
        self.tasks.get(id).unwrap()
    }

    fn get_node_mut(&mut self, id: NodeId) -> NodeMut<'_, Task> {
        self.tasks.get_mut(id).unwrap()
    }

    pub fn get_task(&self, id: NodeId) -> &Task {
        self.get_node(id).data()
    }

    pub fn set_task(&mut self, id: NodeId, task: Task) {
        *self.get_node_mut(id).data() = task;
    }

    pub fn get_parent_non_root(&self, id: NodeId) -> Option<NodeId> {
        if self.is_top_level(id) { None }
        else { Some(self.get_node(id).parent().unwrap().node_id()) }
    }

    pub fn get_first_child(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).first_child().map(|child| child.node_id())
    }

    pub fn get_prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).prev_sibling().map(|sib| sib.node_id())
    }

    pub fn get_next_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).next_sibling().map(|sib| sib.node_id())
    }

    pub fn get_above(&self, id: NodeId) -> Option<NodeId> {
        let node = self.get_node(id);
        if let Some(sibling) = node.prev_sibling() {
            if let Some(descendant) = sibling.traverse_pre_order().last() { Some(descendant.node_id()) }
            else { Some(sibling.node_id()) }
        }
        else { self.get_parent_non_root(id) }
    }

    pub fn get_below(&self, id: NodeId) -> Option<NodeId> {
        let node = self.get_node(id);
        if let Some(child) = node.first_child() { Some(child.node_id()) }
        else if let Some(sibling) = node.next_sibling() { Some(sibling.node_id()) }
        // TODO Keep walking up ancestors
        else { node.parent().unwrap().next_sibling().map(|pibling| pibling.node_id()) }
    }

    pub fn switch_with_prev_sibling(&mut self, id: NodeId) -> Option<NodeId> {
        if self.get_node_mut(id).swap_prev_sibling() { Some(id) }
        else { None }
    }

    pub fn switch_with_next_sibling(&mut self, id: NodeId) -> Option<NodeId> {
        if self.get_node_mut(id).swap_next_sibling() { Some(id) }
        else { None }
    }

    fn is_root(&self, id: NodeId) -> bool {
        id == self.tasks.root_id().unwrap()
    }

    fn is_top_level(&self, id: NodeId) -> bool {
        self.get_node(id).parent().unwrap().node_id() == self.tasks.root_id().unwrap()
    }

    pub fn has_children(&self, id: NodeId) -> bool {
        self.get_node(id).first_child().is_some()
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
        if node.data().completed { return false; }

        // If any previous sibling not done, then id is not first actionable
        for sib in node.parent().unwrap().children() {
            if sib.node_id() == id { break; }
            if !sib.data().completed { return false; }
        }
        true
    }

    pub fn add_top_level(&mut self) -> NodeId {
        let task = Task::default();
        self.tasks.root_mut().unwrap().prepend(task).node_id()
    }

    pub fn add_sibling_below(&mut self, id: NodeId) -> NodeId {
        // Create sibling node
        let task = Task::default();
        let parent = self.get_node(id).parent().unwrap().node_id();
        let added_id = self.get_node_mut(parent).append(task).as_ref().node_id();

        // Move it to under id
        while let Some(above) = self.get_node(added_id).prev_sibling() {
            if above.node_id() == id {
                break;
            }
            self.get_node_mut(added_id).swap_prev_sibling();
        }
        added_id
    }

    pub fn add_sibling_above(&mut self, id: NodeId) -> NodeId {
        let added_id = self.add_sibling_below(id);
        self.get_node_mut(added_id).swap_prev_sibling();
        added_id
    }

    pub fn add_child(&mut self, id: NodeId) -> NodeId {
        let task = Task::default();
        self.get_node_mut(id).prepend(task).node_id()
    }

    pub fn remove(&mut self, id: NodeId) -> NodeId {
        if self.is_root(id) { return id; }
        let node = self.get_node(id);
        let next_selected = node.prev_sibling().unwrap_or(
            node.next_sibling().unwrap_or(
                node.parent().unwrap()
            )
        ).node_id();
        self.tasks.remove(id, DropChildren).unwrap();
        next_selected
    }

    fn set_descendants_completion(&mut self, id: NodeId) {
        let node = self.get_node(id);
        let completed = node.data().completed;
        let descendants: Vec<NodeId> = node.traverse_pre_order().map(|node| node.node_id()).collect();
        for id in descendants {
            self.get_node_mut(id).data().completed = completed;
        }
    }

    fn update_ancestors_completion(&mut self, id: NodeId) {
        let node = self.get_node(id);
        let ancestors: Vec<NodeId> = node.ancestors().map(|node| node.node_id()).collect();
        for id in ancestors {
            let all_children_completed = self.get_node(id)
                .children()
                .all(|child| child.data().completed);
            self.get_node_mut(id).data().completed = all_children_completed;
        }
    }

    pub fn toggle_completed(&mut self, id: NodeId) {
        if self.is_root(id) { return; }
        // Toggle id
        let mut node = self.get_node_mut(id);
        node.data().completed = !node.data().completed;
        // Update descendants
        self.set_descendants_completion(id);
        // Update ancestors
        self.update_ancestors_completion(id);
    }

    pub fn set_title(&mut self, id: NodeId, title: String) {
        if self.is_root(id) { return; }
        self.get_node_mut(id).data().title = title;
    }
}
