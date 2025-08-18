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

            // find the first comment marker anywhere in the line
            let mut comment_scan_index = 0;
            let mut found_comment = None;

            while comment_scan_index < line.len() {
                let rel = memchr::memchr3(b'#', b'/', b'-', &line[comment_scan_index..]);

                let rel = match rel {
                    Some(i) => i,
                    None => break,
                };

                let index = comment_scan_index + rel;

                // safe to slice at `idx` because memchr found an ASCII byte
                if let Some(marker_len) = util::is_line_a_comment(&line_str[index..]) {
                    found_comment = Some((index, marker_len));
                    break
                }

                comment_scan_index = index + 1; // continue searching after the found byte
            }

            let (rel_comment_start, marker_len) = match found_comment {
                Some(v) => v,
                None => continue // no comment on this line
            };

            // the part after the comment marker, BEFORE trimming spaces
            let rest_after_mark = &line_str[rel_comment_start + marker_len..];
            let content = rest_after_mark.trim_start(); // content after marker and any spaces

            let ws_after_marker = rest_after_mark.len() - content.len();

            // helper: find TODO inside a byte slice using memchr
            fn position_todo(haystack: &[u8], start: usize) -> Option<usize> {
                let mut pos = start;
                while let Some(i) = memchr::memchr(b'T', &haystack[pos..]) {
                    let index = pos + i;
                    if haystack[index..].starts_with(b"TODO") {
                        return Some(index)
                    }
                    pos = index + 1;
                }

                None
            }

            let content_bytes = content.as_bytes();
            let Some(todo_idx_in_content) = position_todo(content_bytes, 0) else {
                continue
            };

            // we require the TODO to be right after the comment (after optional whitespace),
            // i.e. at the start of `content`.
            if todo_idx_in_content != 0 {
                // TODO is present but not immediately after the comment marker -> skip
                continue
            }

            // todo_slice starts at the "TODO" occurrence (within trimmed content)
            let todo_slice = &content[todo_idx_in_content..];

            let is_untagged = todo_slice.starts_with("TODO:");
            let (title, is_tagged) = Todo::extract_todo_title(todo_slice);

            if title.trim().is_empty() { continue }

            if !is_tagged && !is_untagged { continue }

            let loc = Loc(file_id, line_number - 1);

            let line_start = byte_offset - line.len();

            // position where to insert tag: compute absolute byte offset in file.
            // line_start + comment_pos = start of comment marker in file
            // + marker_len + ws_after_marker = start of `content` in file
            // + todo_idx_in_content = start of TODO in content
            // + "TODO".len() = position after the word TODO
            let tag_insertion_offset = line_start
                + rel_comment_start
                + marker_len
                + ws_after_marker
                + todo_idx_in_content
                + "TODO".len();

            let (
                description,
                description_line_end
            ) = Todo::extract_todo_description(
                &haystack[byte_offset..]
            ).map_or((None, None), |(d, l)| (Some(d), Some(l)));

            let todo = Todo {
                loc,
                description,
                tag_insertion_offset,
                preview: util::string_into_boxed_str_norealloc(content.to_owned()),
                title: util::string_into_boxed_str_norealloc(title.to_owned()),
            };

            match self.config.mode {
                Mode::Reporting => if is_untagged {
                    self.found_count.fetch_add(1, Ordering::SeqCst);

                    mode_value.push_todo(todo);
                }

                Mode::Purging => if is_tagged {
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

                    let mut comment_ws_pos = rel_comment_start;
                    while comment_ws_pos > 0 && line[comment_ws_pos - 1] == b' ' {
                        comment_ws_pos -= 1
                    }

                    let global_comment_start = line_start + comment_ws_pos;

                    let line_end = description_line_end.map(|dl| {
                        dl + byte_offset
                    }).unwrap_or(line_end);

                    let global_comment_end = if rel_comment_start == 0 {
                        // include newline in this line for the purge
                        line_end
                    } else {
                        // don't include
                        line_end.saturating_sub(1)
                    };

                    mode_value.push_purge(Purge {
                        tag: Tag { todo, issue_number },
                        range: global_comment_start..global_comment_end
                    });
                }

                Mode::Listing => unimplemented!()
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
