use crate::{comment::Comment, util};
use crate::loc::Loc;

use std::{fmt, str};

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
            return None
        }

        // find closing ')' that ends the TODO(...)
        if let Some(end_paren) = memchr::memchr(b')', &bytes[1..]) {
            let after_paren = &s[end_paren + 2..]; // +1 for offset, +1 for ')'
            if let Some(stripped) = after_paren.strip_prefix(':') {
                return Some(stripped.trim_start()) // skip ':'
            }
        }

        None
    }

    /// Returns: (Description, index of the last newline in the last descriptionl line)
    #[inline]
    pub fn extract_todo_description(
        h: &[u8],
        comment: Comment
    ) -> Option<(Description, usize)> {
        let mut lines = Vec::with_capacity(4);

        let mut start = 0;
        let mut end   = 0;

        while start < h.len() {
            // find next newline
            let nl_rel = memchr::memchr(b'\n', &h[start..]);
            let line_end = match nl_rel {
                Some(rel) => start + rel + 1, // include '\n'
                None      => h.len(),        // last line w/o '\n'
            };

            let line = &h[start..line_end];

            start = line_end;

            let line_str = unsafe {
                str::from_utf8_unchecked(line)
            };

            if comment.is_line_a_comment(line_str).is_none() {
                break
            }

            let line_str = util::trim_comment_start(line_str);

            if line_str.is_empty() {
                break
            }

            if ["TODO:", "TODO("].iter().any(|p| line_str.starts_with(p)) {
                break
            }

            end = line_end;

            let line_str = line_str.strip_suffix('\n').unwrap_or(line_str);

            let line_str = util::string_into_boxed_str_norealloc(
                line_str.to_owned()
            );

            lines.push(line_str);
        }

        if lines.is_empty() {
            return None
        }

        let lines = util::vec_into_boxed_slice_norealloc(lines);
        Some((Description { lines }, end))
    }
}
