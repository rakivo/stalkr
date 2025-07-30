use crate::util;
use crate::loc::Loc;

use std::fmt;

pub type Todos = Box<[Todo]>;

#[derive(Debug)]
pub struct Description {
    pub lines: Box<[Box<str>]>
}

impl Description {
    #[inline]
    pub fn from_str(s: &str) -> Self {
        let lines = s.lines().map(|l| {
            util::string_into_boxed_str_norealloc(l.trim().to_owned())
        }).collect();

        Self { lines }
    }

    #[inline(always)]
    pub const fn display(&self, line_start_offset: usize) -> DisplayDescription<'_> {
        DisplayDescription { desc: self, line_start_offset }
    }
}

pub struct DisplayDescription<'a> {
    desc: &'a Description,
    line_start_offset: usize
}

impl fmt::Display for DisplayDescription<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tab = " ".repeat(self.line_start_offset);

        for (i, l) in self.desc.lines.iter().enumerate() {
            write!(f, "{tab}{l}")?;
            if i < self.desc.lines.len() - 1 { writeln!(f)? }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Todo {
    pub loc: Loc,
    #[allow(unused)]
    pub preview: Box<str>,
    pub title: Box<str>,
    pub tag_insertion_offset: usize,
    pub description: Option<Description>
}

impl Todo {
    #[inline]
    pub fn as_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title,
            "body": self.description.as_ref().map(|ls| ls.lines.join("\n"))
        })
    }

    #[inline]
    pub fn extract_todo_title(h: &str) -> &str {
        util::trim_comment_start(h)
            .trim_start_matches("TODO:")
            .trim()
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
