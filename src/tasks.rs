#[derive(Debug, Clone, Default, PartialEq)]
pub struct Task {
    pub title: String,
    pub indent: usize,
    pub completed: bool,
}

impl Task {
    pub fn from_str(string: &str, indent_width: usize) -> Option<Task> {
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
        let indent = (string.len() - trimmed.len()) / indent_width;
        Some(Task {
            title,
            completed,
            indent,
        })
    }

    pub fn to_str(&self, indent_width: usize) -> String {
        let whitespace = " ".repeat(self.indent * indent_width);
        let marker = if self.completed { "x" } else { " " };
        format!("{}- [{}] {}", whitespace, marker, self.title)
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    pub fn toggle_completed(&mut self) {
        self.completed = !self.completed
    }
}

pub fn serialize_tasks(tasks: &[Task], indent_width: usize) -> String {
    tasks.iter().fold(String::new(), |mut acc, task| {
        let line = task.to_str(indent_width) + "\n";
        acc.push_str(&line);
        acc
    })
}
