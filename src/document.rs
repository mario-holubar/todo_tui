use crate::tasks::TaskTree;
use ego_tree::NodeId;

#[derive(Debug)]
pub struct Tab {
    pub name: String,
    pub tasks: TaskTree,
    pub selection: NodeId,
}

impl Tab {
    pub fn new(name: String, indent_width: usize) -> Self {
        let (tasks, selection) = TaskTree::from_string("", indent_width);
        Self { name, tasks, selection }
    }
}

pub fn parse(content: &str, indent_width: usize) -> Vec<Tab> {
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in content.lines() {
        if sections.is_empty() && line.trim().is_empty() { continue; }
        if let Some(name) = line.strip_prefix("# ").filter(|name| !name.trim().is_empty()) {
            sections.push((name.trim().to_string(), String::new()));
        } else {
            if sections.is_empty() {
                sections.push(("todo".to_string(), String::new()));
            }
            sections.last_mut().unwrap().1.push_str(line);
            sections.last_mut().unwrap().1.push('\n');
        }
    }
    if sections.is_empty() {
        sections.push(("todo".to_string(), String::new()));
    }
    sections.into_iter().map(|(name, body)| {
        let (tasks, selection) = TaskTree::from_string(&body, indent_width);
        Tab { name, tasks, selection }
    }).collect()
}

pub fn serialize(tabs: &[Tab], indent_width: usize) -> String {
    tabs.iter()
        .map(|tab| serialize_section(&tab.name, &tab.tasks, indent_width))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn serialize_section(name: &str, tasks: &TaskTree, indent_width: usize) -> String {
    let body = tasks.to_string(indent_width);
    if body.trim().is_empty() {
        format!("# {name}\n")
    } else {
        format!("# {name}\n{body}")
    }
}
