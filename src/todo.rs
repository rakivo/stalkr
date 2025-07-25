use crate::loc::Loc;

pub const TODO_REGEXP: &str = r"(?m)^\s*(?://|#|/\*)\s*TODO:\s*(.+)$";

#[derive(Debug)]
pub struct Todo {
    pub loc: Loc,
    pub title: String,
}

