use crate::tag::Tag;

use std::hint;
use std::path::Path;
use std::io::{self, Read};
use std::fs::{self, File};
use std::sync::atomic::{AtomicU32, Ordering};

use rustc_hash::FxBuildHasher;
use dashmap::{DashMap, DashSet};
use memmap2::{MmapMut, MmapOptions};
use dashmap::mapref::one::{Ref, RefMut, MappedRef, MappedRefMut};

pub type FxDashSet<V>    = DashSet<V, FxBuildHasher>;
pub type FxDashMap<K, V> = DashMap<K, V, FxBuildHasher>;

type FileRef<'a>    = Ref<'a, FileId, StalkrFile>;
type FileRefMut<'a> = RefMut<'a, FileId, StalkrFile>;

pub type FilePathGuard<'a> = MappedRef<'a, FileId, StalkrFile, String>;

type MmapGuardMut<'a> = MappedRefMut<'a, FileId, StalkrFile, MmapMut>;

#[derive(Eq, Hash, Copy, Clone, Debug, PartialEq)]
pub struct FileId(u32);

#[derive(Debug)]
pub enum StalkrFileContents {
    Buf(Vec<u8>),
    Mmap(MmapMut)
}

impl StalkrFileContents {
    #[track_caller]
    #[inline(always)]
    #[must_use] 
    pub fn as_buf_unchecked(&self) -> &Vec<u8> {
        match self {
            Self::Buf(b) => b,
            _ => unsafe { hint::unreachable_unchecked() }
        }
    }

    #[track_caller]
    #[inline(always)]
    #[must_use] 
    pub fn as_mmap_unchecked(&self) -> &MmapMut {
        match self {
            Self::Mmap(m) => m,
            _ => unsafe { hint::unreachable_unchecked() }
        }
    }

    #[track_caller]
    #[inline(always)]
    pub fn as_mmap_unchecked_mut(&mut self) -> &mut MmapMut {
        match self {
            Self::Mmap(m) => m,
            _ => unsafe { hint::unreachable_unchecked() }
        }
    }
}

#[derive(Debug)]
pub struct StalkrFile {
    // user path (not canonicalized)
    pub upath: String,

    pub meta: fs::Metadata,

    pub handle: File,

    pub tags: Vec<Tag>,

    contents: Option<StalkrFileContents>
}

impl StalkrFile {
    #[inline(always)]
    #[must_use] 
    pub fn new(upath: String, handle: File, meta: fs::Metadata) -> Self {
        Self { meta, upath, handle, tags: Vec::new(), contents: None }
    }

    #[inline(always)]
    #[must_use] 
    pub fn read_contents_unchecked(&self) -> &StalkrFileContents {
        unsafe { self.contents.as_ref().unwrap_unchecked() }
    }

    #[inline(always)]
    pub fn read_contents_unchecked_mut(&mut self) -> &mut StalkrFileContents {
        unsafe { self.contents.as_mut().unwrap_unchecked() }
    }

    #[inline]
    pub fn read_file_to_vec(&mut self) -> io::Result<&[u8]> {
        let file_size = self.meta.len() as usize;

        match self.contents {
            Some(StalkrFileContents::Buf(_)) => {}
            Some(StalkrFileContents::Mmap(_)) => unreachable!{
                "`read_file_to_end` called on a mmapped file"
            },

            None => {
                let mut buf = Vec::with_capacity(file_size);
                self.handle.read_to_end(&mut buf)?;
                self.contents = Some(StalkrFileContents::Buf(buf));
            }
        }

        Ok(self.read_contents_unchecked().as_buf_unchecked())
    }

    #[inline]
    pub fn mmap_file(&mut self) -> io::Result<&MmapMut> {
        if let Some(StalkrFileContents::Mmap(_)) = &self.contents {
            return Ok(self.read_contents_unchecked().as_mmap_unchecked())
        }

        if self.contents.is_none() {
            let mut opts = MmapOptions::new();
            opts.len(self.meta.len() as usize);

            let mmap = unsafe { opts.map_mut(&self.handle)? };

            self.contents = Some(StalkrFileContents::Mmap(mmap));
        }

        Ok(self.read_contents_unchecked().as_mmap_unchecked())
    }
}

#[derive(Debug, Default)]
pub struct FileManager {
    pub files: FxDashMap<FileId, StalkrFile>,

    file_id: AtomicU32,

    // seen canonicalized filepaths
    seen: FxDashSet<String>,
}

impl FileManager {
    #[track_caller]
    #[inline(always)]
    pub fn get_file_unchecked(&self, file_id: FileId) -> FileRef<'_> {
        self.files.get(&file_id).unwrap()
    }

    #[track_caller]
    #[inline(always)]
    pub fn get_file_unchecked_mut(&self, file_id: FileId) -> FileRefMut<'_> {
        self.files.get_mut(&file_id).unwrap()
    }

    #[track_caller]
    #[inline(always)]
    pub fn get_file_path_unchecked(&self, file_id: FileId) -> FilePathGuard<'_> {
        self.get_file_unchecked(file_id).map(|f| &f.upath)
    }

    #[inline(always)]
    pub fn add_tag_to_file(&self, file_id: FileId, tag: Tag) {
        self.get_file_unchecked_mut(file_id).tags.push(tag);
    }

    pub fn get_mmap_or_remmap_file_mut(
        &self,
        file_id: FileId,
        new_len: usize,
    ) -> io::Result<MmapGuardMut<'_>> {
        let mut entry = self.get_file_unchecked_mut(file_id);

        let orig_len = entry.meta.len() as usize;

        if orig_len == new_len && matches!(entry.contents, Some(StalkrFileContents::Mmap(_))) {
            // no need to remmap
            return Ok(entry.map(|f| f.read_contents_unchecked_mut().as_mmap_unchecked_mut()))
        }

        entry.handle.set_len(new_len as _)?;

        let mut opts = MmapOptions::new();
        opts.len(new_len);

        let mmap = unsafe { opts.map_mut(&entry.handle)? };

        entry.contents = Some(StalkrFileContents::Mmap(mmap));

        Ok(entry.map(|f| f.read_contents_unchecked_mut().as_mmap_unchecked_mut()))
    }

    #[inline]
    pub fn next_file_id(&self) -> FileId {
        let id = self.file_id.fetch_add(1, Ordering::SeqCst);
        FileId(id)
    }

    /// Returns true if file is not seen
    #[inline]
    pub fn mark_seen(&self, uncanonicalized: &Path) -> bool {
        let Ok(canonicalized) = fs::canonicalize(uncanonicalized) else {
            eprintln!{
                "[could not canonicalize file path]: {u}",
                u = uncanonicalized.display()
            };

            return false
        };

        let s = canonicalized.to_string_lossy().into_owned();
        self.seen.insert(s)
    }

    #[inline]
    pub fn register_stalkr_file(
        &self,
        file: StalkrFile,
        file_id: FileId
    ) {
        self.files.insert(file_id, file);
    }
}
