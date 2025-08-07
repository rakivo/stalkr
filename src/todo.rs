use crate::util;
use crate::loc::Loc;

use std::fmt;

#[derive(Debug)]
pub struct Description {
    pub lines: Box<[Box<str>]>
}

impl Description {
    #[inline]
    #[allow(clippy::should_implement_trait)]
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

    /// Returns: (todo's title, is todo tagged or not)
    #[inline]
    pub fn extract_todo_title(h: &str) -> (&str, bool) {
        let mut s = util::trim_comment_start(h).trim_start();
        let mut is_tagged = false;

        if let Some(rest) = s.strip_prefix("TODO") {
            let rest = rest.trim_start();

            if let Some(after_colon) = rest.strip_prefix(':') {
                // e.g. "TODO:"
                s = after_colon.trim_start();
            } else if let Some(stripped) = Self::strip_todo_parens(rest) {
                // e.g. "TODO(<...>):"
                s = stripped;
                is_tagged = true;
            }
        }

        // trailing "*/"
        s = s.trim_end_matches("*/").trim();

        (s, is_tagged)
    }

    // Helper: parse TODO(<...>): and return what's after it
    #[inline]
    fn strip_todo_parens(s: &str) -> Option<&str> {
        let bytes = s.as_bytes();
        if bytes.first() != Some(&b'(') {
            return None;
        }

        // Find closing ')' that ends the TODO(...)
        if let Some(end_paren) = memchr::memchr(b')', &bytes[1..]) {
            let after_paren = &s[end_paren + 2..]; // +1 for offset, +1 for ')'
            if after_paren.starts_with(':') {
                return Some(after_paren[1..].trim_start()); // skip ':'
            }
        }

        None
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
