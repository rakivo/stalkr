use std::io::{self, Read};
use std::fs::{self, File};
use std::sync::atomic::{AtomicU32, Ordering};

use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use memmap2::{Mmap, MmapOptions};
use dashmap::mapref::one::{MappedRef, Ref, RefMut};

pub type FileId = u32;

pub type FxDashMap<K, V> = DashMap<K, V, FxBuildHasher>;

type FileRef<'a>    = Ref<'a, FileId, StalkrFile>;
type FileRefMut<'a> = RefMut<'a, FileId, StalkrFile>;

type BufGuard<'a>  = MappedRef<'a, FileId, StalkrFile, Vec<u8>>;
type MmapGuard<'a> = MappedRef<'a, FileId, StalkrFile, Mmap>;

#[derive(Debug)]
pub enum StalkrFileContents {
    Buf(Vec<u8>),
    Mmap(Mmap)
}

impl StalkrFileContents {
    #[track_caller]
    #[inline(always)]
    pub fn as_buf_unchecked(&self) -> &Vec<u8> {
        match self { Self::Buf(b) => b, _ => unreachable!() }
    }

    #[track_caller]
    #[inline(always)]
    pub fn as_mmap_unchecked(&self) -> &Mmap {
        match self { Self::Mmap(m) => m, _ => unreachable!() }
    }
}

#[derive(Debug)]
pub struct StalkrFile {
    // user path (not canonicalized)
    #[allow(unused)]
    upath: String,

    handle: File,

    meta: fs::Metadata,

    contents: Option<StalkrFileContents>,
}

impl StalkrFile {
    #[inline(always)]
    pub fn new(upath: String, handle: File, meta: fs::Metadata) -> Self {
        Self { upath, handle, meta, contents: None }
    }

    #[inline(always)]
    pub fn contents_unchecked(&self) -> &StalkrFileContents {
        unsafe { self.contents.as_ref().unwrap_unchecked() }
    }
}

#[derive(Debug, Default)]
pub struct FileManager {
    files: FxDashMap<FileId, StalkrFile>,

    // canonicalized_file_path -> file_id
    file_id_map: FxDashMap<String, FileId>,
}

impl FileManager {
    #[allow(unused)]
    #[inline(always)]
    pub fn get_file_unchecked(&self, file_id: FileId) -> FileRef<'_> {
        self.files.get(&file_id).unwrap()
    }

    #[inline(always)]
    fn get_file_unchecked_mut(&self, file_id: FileId) -> FileRefMut<'_> {
        self.files.get_mut(&file_id).unwrap()
    }

    pub fn read_file_to_end(&self, file_id: FileId) -> io::Result<BufGuard<'_>> {
        #[inline]
        fn get_buf(e: FileRefMut<'_>) -> BufGuard<'_> {
            e.downgrade().map(|f| f.contents_unchecked().as_buf_unchecked())
        }

        let mut entry = self.get_file_unchecked_mut(file_id);

        if let Some(StalkrFileContents::Buf(_)) = &entry.contents {
            return Ok(get_buf(entry))
        }

        let file_size = entry.meta.len() as usize;

        let mut buf = Vec::with_capacity(file_size);

        // it's ok to mutate entry.handle's cursor because we only mutate it once
        _ = entry.handle.read_to_end(&mut buf)?;

        entry.contents = Some(StalkrFileContents::Buf(buf));

        Ok(get_buf(entry))
    }

    pub fn mmap_file(&self, file_id: FileId) -> io::Result<MmapGuard<'_>> {
        let mut entry = self.get_file_unchecked_mut(file_id);

        if entry.contents.is_none() {
            let mut opts = MmapOptions::new();

            let file_size = entry.meta.len() as usize;
            opts.len(file_size);

            let mmap = unsafe { opts.map(&entry.handle) }?;

            entry.contents = Some(StalkrFileContents::Mmap(mmap))
        }

        let mmap = entry.downgrade().map(|f| f.contents_unchecked().as_mmap_unchecked());

        Ok(mmap)
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

        let file_id = CURR_MODULE_ID.fetch_add(1, Ordering::SeqCst);

        self.file_id_map.insert(canonicalized, file_id);

        file_id
    }
}
