use crate::loc::Loc;

pub const TODO_REGEXP: &str = r"(?m)^\s*(?://|#|/\*)\s*TODO:\s*(.+)$";

#[derive(Debug)]
pub struct Todo {
    pub loc: Loc,
    pub title: String,
}

impl Todo {
    #[inline]
    pub fn from_preview_and_loc(preview: &str, loc: Loc) -> Self {
        let title = Self::extract_todo_title(preview).to_owned();
        Self { loc, title }
    }

    #[inline]
    fn extract_todo_title(preview: &str) -> &str {
        preview.trim_start()
            .trim_start_matches("//")
            .trim_start_matches('#')
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim_start()
            .strip_prefix("TODO:")
            .unwrap_or(preview)
            .trim()
    }
}
