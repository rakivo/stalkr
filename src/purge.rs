use crate::loc::Loc;
use crate::fm::{FileId, FileManager};

use std::fs::OpenOptions;
use std::ops::{Range, Deref, DerefMut};
use std::sync::atomic::{Ordering, AtomicUsize};

pub struct Purge {
    pub loc: Loc,
    pub issue_number: u64,
    pub range: Range<usize>
}

pub struct Purges {
    pub file_id: FileId,
    pub purges: Vec<Purge>,
}

impl Deref for Purges {
    type Target = Vec<Purge>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target { &self.purges }
}

impl DerefMut for Purges {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.purges }
}

impl Purges {
    #[inline(always)]
    pub fn with_capacity(n: usize, file_id: FileId) -> Self {
        Self { file_id, purges: Vec::with_capacity(n) }
    }

    pub fn apply(
        mut self,
        processed_count: &AtomicUsize,
        fm: &FileManager
    ) -> anyhow::Result<()> {
        if self.is_empty() {
            return Ok(())
        }

        self.purges.sort_by_key(|p| p.range.start);

        let mut new_len = fm.get_file_unchecked(self.file_id).meta.len() as usize;

        let file_path = fm.get_file_path_unchecked(self.file_id).to_owned();
        let mut mmap = fm.get_mmap_or_remmap_file_mut(self.file_id, new_len)?;

        for Purge { range, .. } in self.purges.into_iter().rev() {
            let start = range.start;
            let end   = range.end;

            debug_assert!{
                end <= new_len,
                "purge range {range:?} past end {new_len}"
            };

            let len = end - start;

            // how many bytes follow this hole right now?
            let tail_len = new_len - end;

            // slide the tail block down on top of the hole
            mmap.copy_within(end..end + tail_len, start);

            processed_count.fetch_add(1, Ordering::SeqCst);

            // reduce the effective length
            new_len -= len;
        }

        // flush and shrink
        mmap.flush()?;
        drop(mmap);

        OpenOptions::new()
            .write(true)
            .open(&file_path)?
            .set_len(new_len as _)?;

        Ok(())
    }
}
