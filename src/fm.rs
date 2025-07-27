use crate::tag::Tag;

use std::io::{self, Read};
use std::fs::{self, File};
use std::sync::atomic::{AtomicU32, Ordering};

use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use memmap2::{MmapMut, MmapOptions};
use dashmap::mapref::one::{Ref, RefMut, MappedRef, MappedRefMut};

pub type FxDashMap<K, V> = DashMap<K, V, FxBuildHasher>;

type FileRef<'a>    = Ref<'a, FileId, StalkrFile>;
type FileRefMut<'a> = RefMut<'a, FileId, StalkrFile>;

pub type FilePathGuard<'a> = MappedRef<'a, FileId, StalkrFile, String>;

type BufGuard<'a>  = MappedRef<'a, FileId, StalkrFile, Vec<u8>>;
type MmapGuard<'a> = MappedRef<'a, FileId, StalkrFile, MmapMut>;
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
    pub fn as_buf_unchecked(&self) -> &Vec<u8> {
        match self { Self::Buf(b) => b, _ => unreachable!() }
    }

    #[track_caller]
    #[inline(always)]
    pub fn as_mmap_unchecked(&self) -> &MmapMut {
        match self { Self::Mmap(m) => m, _ => unreachable!() }
    }

    #[track_caller]
    #[inline(always)]
    pub fn as_mmap_unchecked_mut(&mut self) -> &mut MmapMut {
        match self { Self::Mmap(m) => m, _ => unreachable!() }
    }
}

#[derive(Debug)]
pub struct StalkrFile {
    // user path (not canonicalized)
    #[allow(unused)]
    pub upath: String,

    pub meta: fs::Metadata,

    pub handle: File,

    pub tags: Vec<Tag>,

    contents: Option<StalkrFileContents>
}

impl StalkrFile {
    #[inline(always)]
    pub fn new(upath: String, handle: File, meta: fs::Metadata) -> Self {
        Self { meta, upath, handle, tags: Vec::new(), contents: None }
    }

    #[inline(always)]
    pub fn read_contents_unchecked(&self) -> &StalkrFileContents {
        unsafe { self.contents.as_ref().unwrap_unchecked() }
    }

    #[inline(always)]
    pub fn read_contents_unchecked_mut(&mut self) -> &mut StalkrFileContents {
        unsafe { self.contents.as_mut().unwrap_unchecked() }
    }
}

#[derive(Debug, Default)]
pub struct FileManager {
    pub files: FxDashMap<FileId, StalkrFile>,

    // canonicalized_file_path -> file_id
    file_id_map: FxDashMap<String, FileId>,
}

impl FileManager {
    #[inline(always)]
    pub fn get_file_unchecked(&self, file_id: FileId) -> FileRef<'_> {
        self.files.get(&file_id).unwrap()
    }

    #[inline(always)]
    pub fn get_file_unchecked_mut(&self, file_id: FileId) -> FileRefMut<'_> {
        self.files.get_mut(&file_id).unwrap()
    }

    #[inline(always)]
    pub fn get_file_path_unchecked(&self, file_id: FileId) -> FilePathGuard<'_> {
        self.get_file_unchecked(file_id).map(|f| &f.upath)
    }

    #[inline(always)]
    pub fn add_tag_to_file(&self, file_id: FileId, tag: Tag) {
        self.get_file_unchecked_mut(file_id).tags.push(tag)
    }

    #[inline(always)]
    pub fn drop_entry(&self, file_id: FileId) {
        self.files.remove(&file_id);
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

    pub fn read_file_to_end(&self, file_id: FileId) -> io::Result<BufGuard<'_>> {
        let mut entry = self.get_file_unchecked_mut(file_id);

        let file_size = entry.meta.len() as usize;

        #[inline]
        fn get_buf(e: FileRefMut<'_>) -> BufGuard<'_> {
            e.downgrade().map(|f| f.read_contents_unchecked().as_buf_unchecked())
        }

        match entry.contents {
            Some(StalkrFileContents::Buf(_)) => {}
            Some(StalkrFileContents::Mmap(_)) => panic!("`read_file_to_end` called on a mmapped file"),

            None => {
                let mut buf = Vec::with_capacity(file_size);
                entry.handle.read_to_end(&mut buf)?;
                entry.contents = Some(StalkrFileContents::Buf(buf))
            }
        }

        Ok(get_buf(entry))
    }

    pub fn mmap_file(&self, file_id: FileId) -> io::Result<MmapGuard<'_>> {
        let mut entry = self.get_file_unchecked_mut(file_id);

        if let Some(StalkrFileContents::Mmap(_)) = &entry.contents {
            return Ok(entry.downgrade().map(|f| f.read_contents_unchecked().as_mmap_unchecked()))
        }

        if entry.contents.is_none() {
            let mut opts = MmapOptions::new();
            opts.len(entry.meta.len() as usize);

            let mmap = unsafe { opts.map_mut(&entry.handle)? };

            entry.contents = Some(StalkrFileContents::Mmap(mmap));
        }

        Ok(entry.downgrade().map(|f| f.read_contents_unchecked().as_mmap_unchecked()))
    }

    #[inline]
    pub fn register_file(
        &self,
        uncanonicalized: &str,
        file: StalkrFile
    ) -> FileId {
        let Ok(canonicalized) = fs::canonicalize(uncanonicalized) else {
            panic!("could not canonicalize file path: {uncanonicalized}");
        };

        let s = canonicalized.to_string_lossy().into_owned();

        if let Some(file_id) = self.file_id_map.get(&s) {
            return *file_id
        }

        let file_id = self.new_file_id(s);
        self.files.insert(file_id, file);

        file_id
    }

    #[inline(always)]
    fn new_file_id(&self, canonicalized: String) -> FileId {
        static CURR_MODULE_ID: AtomicU32 = AtomicU32::new(0);
        let file_id = FileId(CURR_MODULE_ID.fetch_add(1, Ordering::SeqCst));
        self.file_id_map.insert(canonicalized, file_id);
        file_id
    }
}
