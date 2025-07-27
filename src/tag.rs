use crate::fm::{FileId, FileManager};

use std::sync::Arc;
use std::{io, mem, fmt};

use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug)]
pub struct Tag {
    pub byte_offset  : u64,
    pub issue_number : u64
}

impl fmt::Display for Tag {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { issue_number, .. } = self;
        write!(f, "(#{issue_number})")
    }
}

pub async fn poll_ready_files(
    mut tag_rx: UnboundedReceiver<FileId>,
    fm: Arc<FileManager>,
    inserter_count: usize,
) {
    let sem = Arc::new(Semaphore::new(inserter_count));

    while let Some(file_id) = tag_rx.recv().await {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let fm_clone = fm.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(err) = insert_tags(file_id, &fm_clone) {
                eprintln!{
                    "[tag] failed to insert tags for file {file_id:?}: {err:#}"
                }
            }

            drop(permit);
        });
    }
}

fn insert_tags(file_id: FileId, fm: &FileManager) -> io::Result<()> {
    let mut insertions = mem::take(
        &mut fm.get_file_unchecked_mut(file_id).tags
    );

    if insertions.is_empty() {
        return Ok(())
    }

    // sort ascending so that all prior inserts were at <= current offset
    insertions.sort_by(|a, b| a.byte_offset.cmp(&b.byte_offset));

    let insertions = insertions.into_iter().map(|t| {
        (t.byte_offset, t.to_string())
    }).collect::<Vec<_>>();

    let total_insert_len = insertions
        .iter()
        .map(|(_, s)| s.len())
        .sum::<usize>();

    let orig_len = fm.get_file_unchecked(file_id).meta.len() as usize;
    let new_len = orig_len + total_insert_len;

    let mut mmap = fm.get_mmap_or_remmap_file_mut(file_id, new_len)?;

    let mut shift = 0;

    for (byte_offset, ref tag) in insertions {
        let byte_offset = byte_offset as usize;
        let insert_bytes = tag.as_bytes();
        let tag_len = insert_bytes.len();

        let actual_offset = byte_offset + shift;
        mmap.copy_within(
            actual_offset..orig_len + shift,
            actual_offset + tag_len,
        );

        mmap[actual_offset..actual_offset + tag_len]
            .copy_from_slice(insert_bytes);

        shift += tag_len;
    }

    mmap.flush()?;

    Ok(())
}
