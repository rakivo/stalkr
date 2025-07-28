use crate::util;
use crate::loc::Loc;

use std::fmt;

pub type Todos = Box<[Todo]>;

pub const TODO_REGEXP: &str = r"(?m)^\s*(?://|#|/\*)\s*TODO:\s*(.+)$";

#[derive(Debug)]
pub struct Description {
    pub lines: Box<[Box<str>]>
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
            writeln!(f, "{tab}{l}")?
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Todo {
    pub loc: Loc,
    pub title: String,
    pub todo_byte_offset: usize,
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

            if ["TODO:", "TODO("].iter().any(|p| line.starts_with(p)) {
                break
            }

            let line = line.to_owned();

            let line = util::string_into_boxed_str_norealloc(line);

            lines.push(line);
        }

        if lines.is_empty() {
            None
        } else {
            let lines = util::vec_into_boxed_slice_norealloc(lines);
            Some(Description { lines })
        }
    }
}
