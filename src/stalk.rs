use crate::util;
use crate::loc::Loc;
use crate::fm::FileId;
use crate::config::Config;
use crate::prompt::Prompt;
use crate::todo::{Todo, Todos};
use crate::fm::{FileManager, StalkrFile};

use std::str;
use std::sync::Arc;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use tokio::task::JoinHandle;
use tokio::sync::mpsc::UnboundedSender;

// read directly anything under 1 MiB; otherwise mmap
#[allow(clippy::identity_op)]
const MMAP_THRESHOLD: usize = 1 * 1024 * 1024;

pub struct Stalkr {
    prompter_tx: UnboundedSender<Prompt>,
    config: Arc<Config>,
    fm: Arc<FileManager>,
    found_count: Arc<AtomicUsize>
}

impl Stalkr {
    pub fn spawn(
        fm: Arc<FileManager>,
        config: Arc<Config>,
        prompter_tx: UnboundedSender<Prompt>,
        found_count: Arc<AtomicUsize>
    ) -> JoinHandle<()> {
        let me = Self::new(fm, config, prompter_tx, found_count);

        tokio::task::spawn_blocking(move || {
            dir_rec::DirRec::new(&*me.config.cwd)
                .filter(|p| Stalkr::filter(p))
                .par_bridge()
                .for_each(|e| _ = me.stalk(e))
        })
    }

    #[inline]
    pub fn new(
        fm: Arc<FileManager>,
        config: Arc<Config>,
        prompter_tx: UnboundedSender<Prompt>,
        found_count: Arc<AtomicUsize>
    ) -> Self {
        Self { fm, config, prompter_tx, found_count }
    }

    pub fn stalk(&self, file_path: PathBuf) -> anyhow::Result<()> {
        if !self.fm.mark_seen(&file_path) {
            return Ok(())
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)?;

        let meta = file.metadata()?;

        let file_size = meta.len() as usize;

        let path_str = &file_path.to_string_lossy();

        let mut stalkr_file = StalkrFile::new(
            path_str.to_string(),
            file,
            meta
        );

        let file_id = self.fm.next_file_id();

        let todos = if file_size < MMAP_THRESHOLD {
            let buf = stalkr_file.read_file_to_vec()?;
            self.search(buf, file_id)
        } else {
            let mmap = stalkr_file.mmap_file()?;
            self.search(&mmap[..], file_id)
        };

        if !todos.is_empty() {
            self.fm.register_stalkr_file(stalkr_file, file_id);

            self.prompter_tx.send(Prompt {
                todos
            }).expect("could not send todos to issue worker");
        }

        Ok(())
    }

    pub fn search(&self, haystack: &[u8], file_id: FileId) -> Todos {
        let mut todos = Vec::with_capacity(4);

        let mut byte_offset = 0;
        let mut line_number = 1;

        while byte_offset < haystack.len() {
            // find next newline
            let nl_rel = memchr::memchr(b'\n', &haystack[byte_offset..]);
            let line_end = match nl_rel {
                Some(rel) => byte_offset + rel + 1, // include '\n'
                None      => haystack.len(),        // last line w/o '\n'
            };

            let line = &haystack[byte_offset..line_end];

            byte_offset = line_end;
            line_number += 1;

            let Ok(line_str) = str::from_utf8(line) else {
                continue
            };

            let trimmed = line_str.trim_start();

            let marker_len = match util::is_line_a_comment(trimmed) {
                Some(n) => n,
                _ => continue
            };

            let content = trimmed[marker_len..].trim_start();

            if !content.starts_with("TODO:") {
                continue
            }

            // compute global byte offset of the 'T' in "TODO"
            let line_start_byte    = byte_offset - line.len();
            let ws_trimmed         = line_str.len() - trimmed.len();         // leading spaces removed
            let rest_after_mark    = &trimmed[marker_len..];                 // before trimming spaces
            let ws_after_marker    = rest_after_mark.len() - content.len();  // spaces removed after marker
            let tag_insertion_offset = line_start_byte + ws_trimmed + marker_len + ws_after_marker + "TODO".len();

            let title = Todo::extract_todo_title(content);
            let description = {
                let desc_start = byte_offset;
                let rest = &haystack[desc_start..];
                Todo::extract_todo_description(unsafe {
                    str::from_utf8_unchecked(rest)
                })
            };

            self.found_count.fetch_add(1, Ordering::SeqCst);

            todos.push(Todo {
                loc: Loc(file_id, line_number - 1),
                tag_insertion_offset,
                preview: util::string_into_boxed_str_norealloc(content.to_owned()),
                title:   util::string_into_boxed_str_norealloc(title.to_owned()),
                description,
            });
        }

        util::vec_into_boxed_slice_norealloc(todos)
    }

    #[inline]
    pub fn filter(e: &Path) -> bool {
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
            .extension()
            .map(|ext| BINARY_EXTENSIONS.contains(ext.as_encoded_bytes()))
            .unwrap_or(true);

        !is_bin
    }
}

