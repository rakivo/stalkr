use crate::util;
use crate::loc::Loc;
use crate::fm::FileId;

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
    #[allow(unused)]
    pub src_loc: Loc,
    pub title: String,
    #[allow(unused)]
    pub src_file_id: FileId,
    pub description: Option<Description>
}

impl Todo {
    #[inline]
    pub fn extract_todo_title(h: &str) -> &str {
        util::trim_comment_start(h)
            .strip_prefix("TODO:")
            .unwrap_or(h)
            .trim_end_matches("*/")
            .trim()
    }

    #[inline]
    pub fn extract_todo_description(h: &str) -> Option<Description> {
        let mut lines = Vec::with_capacity(4);

        for line in h.lines() {
            if util::is_line_a_comment(line).is_none() { break }

            let line = util::trim_comment_start(line);

            if line.is_empty() { continue }

            if line.starts_with("TODO:") { break }

            lines.push(line.to_owned())
        }

        if lines.is_empty() {
            None
        } else {
            Some(Description { lines })
        }
    }
}
