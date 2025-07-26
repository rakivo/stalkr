use crate::todo::Todo;
use crate::fm::{FileManager, StalkrFile};
use crate::search::SearchCtx;

use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use tokio::sync::mpsc::UnboundedSender;

// read directly anything under 1 MiB; otherwise mmap
#[allow(clippy::identity_op)]
const MMAP_THRESHOLD: usize = 1 * 1024 * 1024;

pub fn stalk(
    file_path: PathBuf,
    search_ctx: &SearchCtx,
    found_count: &AtomicUsize,
    tx: &UnboundedSender<Todo>,
    fm: &FileManager
) -> anyhow::Result<()> {
    let file = File::open(&file_path)?;

    let meta = file.metadata()?;

    let file_size = meta.len() as usize;

    let path_str = &file_path.to_string_lossy();

    let stalkr_file = StalkrFile::new(
        path_str.to_string(),
        file,
        meta
    );

    let file_id = fm.register_file(path_str, stalkr_file);

    let search = |haystack: &[u8]| {
        search_ctx.search(haystack, found_count, tx, fm, file_id)
    };

    if file_size < MMAP_THRESHOLD {
        let buf = fm.read_file_to_end(file_id)?;
        search(&buf)?;
    } else {
        let mmap = fm.mmap_file(file_id)?;
        search(&mmap[..])?;
    }

    Ok(())
}

