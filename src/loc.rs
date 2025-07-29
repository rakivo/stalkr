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

// per-haystack cache to compute byte_index -> Loc
pub struct LocCache {
    last_byte_index: usize,
    last_row: usize,
}

impl LocCache {
    #[inline(always)]
    pub const fn new() -> Self {
        Self { last_byte_index: 0, last_row: 1, }
    }

    #[inline]
    pub fn get_loc(&mut self, haystack: &[u8], byte_index: usize, file_id: FileId) -> Loc {
        debug_assert!(byte_index > self.last_byte_index, "sequential access expected");

        let additional_newlines = bytecount::count(
            &haystack[self.last_byte_index..byte_index],
            b'\n'
        );

        let row = self.last_row + additional_newlines;

        // update cache for next call
        self.last_byte_index = byte_index;
        self.last_row = row;

        Loc(file_id, row as _)
    }
}
