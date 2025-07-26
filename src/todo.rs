use crate::loc::Loc;

use std::fmt;

pub const TODO_REGEXP: &str = r"(?m)^\s*(?://|#|/\*)\s*TODO:\s*(.+)$";

#[derive(Debug)]
pub struct Description {
    pub lines: Vec<String>
}

impl Description {
    #[inline]
    pub fn display(&self, line_start_offset: usize) -> DisplayDescription<'_> {
        DisplayDescription {
            desc: self,
            line_start_offset
        }
    }
}

pub struct DisplayDescription<'a> {
    desc: &'a Description,
    line_start_offset: usize
}

impl fmt::Display for DisplayDescription<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tab = " ".repeat(self.line_start_offset);

        for l in &self.desc.lines {
            write!(f, "{tab}{l}")?
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Todo {
    pub loc: Loc,
    pub title: String,
    pub description: Option<Description>
}

impl Todo {
    #[inline]
    pub fn extract_todo_title(h: &str) -> &str {
        h
            .trim_start()
            .trim_start_matches("//")
            .trim_start_matches('#')
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim_start()
            .strip_prefix("TODO:")
            .unwrap_or(h)
            .trim()
    }

    #[inline]
    pub fn extract_todo_description(h: &str) -> Option<Description> {
        let mut lines = Vec::new();

        for line in h.lines() {
            let line = line.trim_start();

            // Only consider comment lines
            let stripped = if let Some(s) = line.strip_prefix("//") {
                s.trim_start()
            } else if let Some(s) = line.strip_prefix("#") {
                s.trim_start()
            } else if let Some(s) = line.strip_prefix("/*") {
                s.trim_start()
            } else {
                break; // non-comment line, stop collecting
            };

            // Stop if the comment contains another TODO
            if stripped.to_uppercase().starts_with("TODO:") {
                break;
            }

            // If line is not empty after stripping comment, collect
            if !stripped.is_empty() {
                lines.push(stripped.to_owned())
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some(Description { lines })
        }
    }
}
