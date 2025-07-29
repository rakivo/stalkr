use crate::util;
use crate::loc::Loc;
use crate::fm::FileId;
use crate::prompt::Prompt;
use crate::todo::{self, Todo};
use crate::fm::{FileManager, StalkrFile};

use std::sync::Arc;
use std::path::PathBuf;
use std::fs::OpenOptions;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use tokio::task::JoinHandle;
use regex_automata::dfa::regex::Regex;
use tokio::sync::mpsc::UnboundedSender;

// read directly anything under 1 MiB; otherwise mmap
#[allow(clippy::identity_op)]
const MMAP_THRESHOLD: usize = 1 * 1024 * 1024;

pub struct Stalkr {
    re: Arc<Regex>,
    prompter_tx: UnboundedSender<Prompt>,
    fm: Arc<FileManager>,
    found_count: Arc<AtomicUsize>
}

impl Stalkr {
    pub fn spawn(
        fm: Arc<FileManager>,
        prompter_tx: UnboundedSender<Prompt>,
        found_count: Arc<AtomicUsize>
    ) -> JoinHandle<()> {
        let me = Self::new(fm, prompter_tx, found_count);

        tokio::task::spawn_blocking(move || {
            dir_rec::DirRec::new(".")
                .filter(Stalkr::filter)
                .par_bridge()
                .for_each(|e| _ = me.stalk(e))
        })
    }

    #[inline]
    pub fn new(
        fm: Arc<FileManager>,
        prompter_tx: UnboundedSender<Prompt>,
        found_count: Arc<AtomicUsize>
    ) -> Self {
        let re = Regex::builder()
            .build(todo::TODO_REGEXP)
            .expect("could not init regex engine");

        let re = Arc::new(re);

        Self { fm, re, prompter_tx, found_count }
    }

    pub fn stalk(&self, file_path: PathBuf) -> anyhow::Result<()> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)?;

        let meta = file.metadata()?;

        let file_size = meta.len() as usize;

        let path_str = &file_path.to_string_lossy();

        let stalkr_file = StalkrFile::new(
            path_str.to_string(),
            file,
            meta
        );

        let file_id = self.fm.register_file(path_str, stalkr_file);

        let any = if file_size < MMAP_THRESHOLD {
            let buf = self.fm.read_file_to_end(file_id)?;
            self.search(&buf, file_id)
        } else {
            let mmap = self.fm.mmap_file(file_id)?;
            self.search(&mmap[..], file_id)
        }?;

        if !any {
            self.fm.drop_entry(file_id);
        }

        Ok(())
    }

    #[inline]
    pub fn search(&self, haystack: &[u8], file_id: FileId) -> anyhow::Result<bool> {
        let line_starts = Loc::precompute(haystack);

        let todos = self.re.find_iter(haystack).filter_map(|mat| {
            let start = mat.start();
            let end   = mat.end().min(haystack.len());
            let bytes = &haystack[start..end];

            let loc = Loc::from_precomputed(&line_starts, start, file_id);

            let preview = str::from_utf8(bytes).unwrap_or("<invalid UTF-8>");

            let title = Todo::extract_todo_title(preview);

            let description = Todo::extract_todo_description(unsafe {
                std::str::from_utf8_unchecked(&haystack[end + 1..])
            });

            // e.g.
            // ... TODO: ...
            //     ^
            let todo_byte_offset = {
                start +
                preview.len() -
                util::trim_comment_start(preview).len() +
                "TODO".len()
            };

            let local_todo_byte_offset = todo_byte_offset - start;

            if bytes[local_todo_byte_offset] == b'(' {
                // since this todo has a tag => it's already reported
                return None
            }

            self.found_count.fetch_add(1, Ordering::SeqCst);

            let todo = Todo {
                loc,
                description,
                todo_byte_offset,
                preview: util::string_into_boxed_str_norealloc(
                    preview.to_owned()
                ),
                title: util::string_into_boxed_str_norealloc(
                    title.to_owned()
                )
            };

            Some(todo)
        }).collect::<Vec<_>>();

        if todos.is_empty() {
            return Ok(false)
        }

        let todos = util::vec_into_boxed_slice_norealloc(todos);

        self.prompter_tx.send(Prompt {
            todos
        }).expect("could not send todos to issue worker");

        Ok(true)
    }

    #[inline]
    pub fn filter(e: &PathBuf) -> bool {
        pub const BINARY_EXTENSIONS: phf::Set::<&[u8]> = phf::phf_set! {
            b"exe", b"dll", b"bin", b"o", b"so", b"a", b"lib", b"elf", b"class",
            b"jar", b"war", b"ear", b"apk", b"msi", b"iso", b"img", b"dmg", b"vmdk",
            b"vhd", b"vdi", b"rom", b"efi", b"sys", b"ko", b"bz2", b"xz", b"7z",
            b"gz", b"zip", b"rar", b"tar", b"arj", b"lz", b"cab", b"deb", b"rpm",
            b"pkg", b"z", b"lzh", b"cpio", b"tgz", b"tbz2", b"tlz", b"txz", b"jpg",
            b"jpeg", b"png", b"gif", b"bmp", b"tiff", b"ico", b"mp3", b"aac", b"wav",
            b"flac", b"ogg", b"wma", b"m4a", b"mp4", b"mkv", b"mov", b"avi", b"wmv",
            b"flv", b"webm", b"3gp", b"m2ts", b"mts", b"ts", b"resx", b"pdb", b"dat",
            b"dll.config", b"exe.config", b"pak", b"binlog", b"woff", b"woff2", b"ttf",
            b"eot", b"db", b"sqlite", b"sqlitedb", b"mdb", b"accdb", b"fdb", b"ndf",
            b"bak", b"ldf", b"mdf", b"bcp", b"db3", b"frm", b"myd", b"ib", b"doc",
            b"docx", b"xls", b"xlsx", b"ppt", b"pptx", b"pdf", b"psd", b"ai", b"eps",
            b"indd", b"sketch", b"xcf", b"raw", b"svg", b"otf", b"swf", b"fla", b"cr2",
            b"nef", b"dng", b"arw", b"orf", b"ptx", b"srf", b"pef", b"sr2", b"raf",
            b"3ds", b"blend", b"fbx", b"obj", b"stl", b"dae", b"mmd", b"lwo", b"c4d",
            b"dxf", b"step", b"iges", b"alembic", b"usd", b"usdaz", b"sbsar", b"vtf",
            b"rlib", b"rmeta", b"d",
        };

        let is_bin = e
            .as_path()
            .extension()
            .map(|ext| BINARY_EXTENSIONS.contains(ext.as_encoded_bytes()))
            .unwrap_or(true);

        !is_bin
    }
}

