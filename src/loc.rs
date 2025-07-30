use crate::fm::{self, FileId, FileManager};

use std::fmt;

#[derive(Debug)]
pub struct Loc(pub FileId, pub u32);

impl Loc {
    #[inline(always)]
    pub const fn file_id(&self) -> FileId { self.0 }

    #[inline(always)]
    #[doc(alias = "row")]
    pub const fn line_number(&self) -> u32 { self.1 }

    #[allow(unused)]
    #[inline(always)]
    pub fn display<'a>(&self, fm: &'a FileManager) -> DisplayLoc<'a> {
        let file_path = fm.get_file_path_unchecked(self.0);
        DisplayLoc { file_path, line_number: self.1 }
    }
}

pub struct DisplayLoc<'a> {
    file_path: fm::FilePathGuard<'a>,
    line_number: u32
}

impl fmt::Display for DisplayLoc<'_> {
    fn fmt(&self, fm: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { file_path, line_number: row } = self;
        write!(fm, "{file_path}:{row}")
    }
}
