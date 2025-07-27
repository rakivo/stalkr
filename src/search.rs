use crate::util;
use crate::loc::Loc;
use crate::todo::Todo;
use crate::fm::{FileId, FileManager};

use std::path::PathBuf;
use std::fmt::Write as FmtWrite;
use std::io::{self, Write as IoWrite};
use std::sync::atomic::{Ordering, AtomicUsize};

use regex_automata::dfa::regex::Regex;
use tokio::sync::mpsc::UnboundedSender;

pub struct SearchCtx {
    regex: Regex
}

impl SearchCtx {
    #[inline]
    pub fn new(pattern_str: &str) -> Self {
        let regex = Regex::builder()
            .syntax(Default::default())
            .thompson(Default::default())
            .build(pattern_str)
            .expect("could not init regex engine");

        SearchCtx { regex }
    }

    #[inline]
    pub fn search(
        &self,
        haystack: &[u8],
        found_count: &AtomicUsize,
        tx: &UnboundedSender<Todo>,
        fm: &FileManager,
        file_id: FileId
    ) -> anyhow::Result<bool> {
        let line_starts = Loc::precompute(haystack);

        let mut stdout_buf = String::new();

        let mut any = false;
        for mat in self.regex.find_iter(haystack) {
            any = true;

            let start = mat.start();
            let end   = mat.end().min(haystack.len());
            let bytes = &haystack[start..end];

            let loc = Loc::from_precomputed(&line_starts, start, file_id);

            let preview = str::from_utf8(bytes).unwrap_or("<invalid UTF-8>");

            let title = Todo::extract_todo_title(preview);

            let desc = Todo::extract_todo_description(unsafe {
                std::str::from_utf8_unchecked(&haystack[end + 1..])
            });

            writeln!(stdout_buf, "found TODO at {l}: {preview}", l = loc.display(fm))?;
            writeln!(stdout_buf, "  title: \"{title}\"")?;
            if let Some(desc) = &desc {
                writeln!(stdout_buf, "  description:\n{d}", d = desc.display(4))?;
            }

            // flush buffered message to stdout under lock
            {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                write!(out, "{stdout_buf}")?;
                stdout_buf.clear();
            }

            found_count.fetch_add(1, Ordering::SeqCst);

            if !util::ask_yn("\nreport it?") {
                continue
            }

            let todo_byte_offset = start +
                preview.len() -
                util::trim_comment_start(preview).len() +
                "TODO".len();

            let todo = Todo {
                src_loc: loc,
                todo_byte_offset,
                src_file_id: file_id,
                description: desc,
                title: title.to_owned(),
            };

            tx.send(todo).expect("could not send todo to issue worker");
        }

        Ok(any)
    }

    #[inline]
    pub fn filter(&self, e: PathBuf) -> Option::<PathBuf> {
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

        if is_bin { None } else { Some(e) }
    }
}
