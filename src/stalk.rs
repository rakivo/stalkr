use crate::util;
use crate::tag::Tag;
use crate::loc::Loc;
use crate::fm::FileId;
use crate::todo::Todo;
use crate::purge::Purge;
use crate::prompt::Prompt;
use crate::config::Config;
use crate::issue::IssueValue;
use crate::mode::{Mode, ModeValue};
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

pub enum StalkrTx {
    Issuer(UnboundedSender<IssueValue>),
    Prompter(UnboundedSender<Prompt>),
}

pub struct Stalkr {
    stalkr_tx: StalkrTx,
    config: Arc<Config>,
    fm: Arc<FileManager>,
    found_count: Arc<AtomicUsize>
}

impl Stalkr {
    pub fn spawn(
        fm: Arc<FileManager>,
        config: Arc<Config>,
        stalkr_tx: StalkrTx,
        found_count: Arc<AtomicUsize>
    ) -> JoinHandle<()> {
        let me = Self::new(fm, config, stalkr_tx, found_count);

        tokio::task::spawn_blocking(move || {
            dir_rec::DirRec::new(&*me.config.cwd)
                .filter(|p| Stalkr::filter(p))
                .par_bridge()
                .for_each(|e| _ = me.stalk(e))
        })
    }

    #[inline(always)]
    pub const fn new(
        fm: Arc<FileManager>,
        config: Arc<Config>,
        stalkr_tx: StalkrTx,
        found_count: Arc<AtomicUsize>
    ) -> Self {
        Self { fm, stalkr_tx, config, found_count }
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

        let mode_value = if file_size < MMAP_THRESHOLD {
            let buf = stalkr_file.read_file_to_vec()?;
            self.search(buf, path_str, file_id)
        } else {
            let mmap = stalkr_file.mmap_file()?;
            self.search(&mmap[..], path_str, file_id)
        };

        if mode_value.is_empty() {
            return Ok(())
        }

        self.fm.register_stalkr_file(stalkr_file, file_id);

        match &self.stalkr_tx {
            StalkrTx::Issuer(issuer_tx) => {
                issuer_tx
                    .send(mode_value)
                    .expect("[could not send todos to issue worker]");
            }

            StalkrTx::Prompter(prompter_tx) => {
                prompter_tx
                    .send(Prompt { mode_value })
                    .expect("[could not send todos to prompter thread]");
            }
        }

        Ok(())
    }

    pub fn search(&self, haystack: &[u8], file_path: &str, file_id: FileId) -> ModeValue {
        let mut mode_value = ModeValue::new(self.config.mode, file_id);

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

            let loc = Loc(file_id, line_number - 1);

            let line_start = byte_offset - line.len();

            let ws_trimmed           = line_str.len() - trimmed.len();        // leading spaces removed
            let rest_after_mark      = &trimmed[marker_len..];                // before trimming spaces
            let ws_after_marker      = rest_after_mark.len() - content.len(); // spaces removed after marker
            let tag_insertion_offset = line_start + ws_trimmed + marker_len + ws_after_marker + "TODO".len();

            let (title, is_tagged) = Todo::extract_todo_title(content);
            let description = {
                let desc_start = byte_offset;
                let rest = &haystack[desc_start..];
                Todo::extract_todo_description(unsafe {
                    str::from_utf8_unchecked(rest)
                })
            };

            // TODO(#30): Properly increment found_count
            self.found_count.fetch_add(1, Ordering::SeqCst);

            let todo = Todo {
                loc,
                description,
                tag_insertion_offset,
                preview: util::string_into_boxed_str_norealloc(content.to_owned()),
                title: util::string_into_boxed_str_norealloc(title.to_owned()),
            };

            if content.starts_with("TODO:") {
                mode_value.push_todo(todo);
            } else if is_tagged && self.config.mode == Mode::Purging {
                let skip = "TODO(#".len();

                // file_id is not yet registered, so use file_path instead
                let display_loc = || loc.display_from_str(file_path);

                let Some(closing_paren_pos) = content[skip..].find(')') else {
                    eprintln!{
                        "[{loc}: error: todo tag without closing paren]",
                        loc = display_loc()
                    };

                    continue
                };

                let Ok(issue_number) = content[skip..skip + closing_paren_pos].parse::<u64>() else {
                    eprintln!{
                        "[{loc}: error: failed to parse issue number]",
                        loc = display_loc()
                    };

                    continue
                };

                mode_value.push_purge(Purge {
                    tag: Tag { todo, issue_number },
                    range: line_start..line_end
                });
            }
        }

        mode_value
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
