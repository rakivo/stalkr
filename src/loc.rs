use crate::fm::{self, FileId, FileManager};

use std::fmt;
use std::ops::Deref;

#[derive(Copy, Clone, Debug)]
pub struct Loc(pub FileId, pub u32);

impl Loc {
    #[inline(always)]
    #[must_use] 
    pub const fn file_id(&self) -> FileId { self.0 }

    #[inline(always)]
    #[doc(alias = "row")]
    #[must_use] 
    pub const fn line_number(&self) -> u32 { self.1 }

    #[inline(always)]
    #[must_use] 
    pub fn display_from_str<'a>(&self, file_path: &'a str) -> DisplayLoc<&'a str> {
        DisplayLoc {
            file_path: FilePathDisplay(file_path),
            line_number: self.1
        }
    }

    #[inline(always)]
    pub fn display<'a>(&self, fm: &'a FileManager) -> DisplayLoc<fm::FilePathGuard<'a>> {
        DisplayLoc {
            file_path: FilePathDisplay(fm.get_file_path_unchecked(self.0)),
            line_number: self.1
        }
    }
}

// wrapper that enables generic display for both `&str` and any other refs
pub struct FilePathDisplay<T: Deref>(pub T);

impl<T: Deref> fmt::Display for FilePathDisplay<T>
where
    T::Target: AsRef<str>
{
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_ref())
    }
}

pub struct DisplayLoc<T: Deref> {
    pub file_path: FilePathDisplay<T>,
    pub line_number: u32
}

impl<T: Deref> fmt::Display for DisplayLoc<T>
where
    T::Target: AsRef<str>
{
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { file_path, line_number: row } = self;
        write!(f, "{file_path}:{row}")
    }
}
