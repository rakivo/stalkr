use crate::todo::Todo;
use crate::search::SearchCtx;

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use memmap2::MmapOptions;
use tokio::sync::mpsc::UnboundedSender;

// read directly anything under 1 MiB; otherwise mmap
const MMAP_THRESHOLD: usize = 1 * 1024 * 1024;

pub fn stalk(
    e: PathBuf,
    search_ctx: &SearchCtx,
    found_count: &AtomicUsize,
    tx: &UnboundedSender<Todo>
) -> anyhow::Result<()> {
    let search = |path: &str, haystack: &[u8]| {
        search_ctx.search(haystack, path, found_count, tx)
    };

    let mut file = File::open(&e)?;

    let file_size = file.metadata()?.len() as usize;

    let path_str = &e.to_string_lossy();

    if file_size < MMAP_THRESHOLD {
        let mut buf = Vec::with_capacity(file_size + 1);

        _ = file.read_to_end(&mut buf)?;

        search(path_str, &buf);
    } else {
        let mut opts = MmapOptions::new();
        opts.len(file_size);

        let mmap = unsafe { opts.map(&file) }?;

        unsafe {
            _ = libc::madvise(
                mmap.as_ptr() as _,
                file_size,
                libc::MADV_SEQUENTIAL
            );
        }

        search(path_str, &mmap[..]);
    }

    Ok(())
}

