use crate::fm::{self, FileId, FileManager};

use std::fmt;

#[derive(Debug)]
pub struct Loc(pub FileId, pub u32, pub u32);

impl Loc {
    const AVERAGE_LINES_COUNT: usize = 256;

    #[inline(always)]
    pub const fn file_id(&self) -> FileId { self.0 }

    #[allow(unused)]
    #[inline(always)]
    #[doc(alias = "row")]
    pub const fn line_number(&self) -> u32 { self.1 }

    #[allow(unused)]
    #[inline(always)]
    #[doc(alias = "col")]
    pub const fn column_number(&self) -> u32 { self.2 }

    // O(log lines_count)
    #[inline]
    pub fn from_precomputed(
        line_starts: &[usize],
        match_byte_index: usize,
        file_id: FileId
    ) -> Self {
        let i = match line_starts.binary_search(&match_byte_index) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) if i >= line_starts.len() => line_starts.len() - 1,
            Err(i) => i - 1,
        };

        let row = i + 1;
        let col = match_byte_index - line_starts[i] + 1;

        Self(file_id, row as _, col as _)
    }

    // O(n)
    #[inline]
    pub fn precompute(h: &[u8]) -> Vec<usize> {
        let mut v = Vec::with_capacity(Self::AVERAGE_LINES_COUNT);
        v.push(0);
        for (i, &b) in h.iter().enumerate() {
            if b == b'\n' { v.push(i + 1); }
        } v
    }

    #[inline(always)]
    pub fn display<'a>(&self, fm: &'a FileManager) -> DisplayLoc<'a> {
        let file_path = fm.get_file_path_unchecked(self.0);
        DisplayLoc { file_path, row: self.1, col: self.2 }
    }
}

pub struct DisplayLoc<'a> {
    file_path: fm::FilePathGuard<'a>,
    row: u32, col: u32
}

impl fmt::Display for DisplayLoc<'_> {
    fn fmt(&self, fm: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { file_path, row, col } = self;
        write!(fm, "{file_path}:{row}:{col}")
    }
}

