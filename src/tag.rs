use crate::todo::Todo;
use crate::config::Config;
use crate::fm::{FileId, FileManager};

use std::sync::Arc;
use std::{io, mem, fmt};

use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug)]
pub struct Tag {
    pub issue_number: u64,
    pub todo: Todo
}

impl fmt::Display for Tag {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { issue_number, .. } = self;
        write!(f, "(#{issue_number})")
    }
}

impl Tag {
    #[inline(always)]
    pub fn commit_msg(&self) -> String {
        format!{
            "Add TODO{self}: {t}",
            t = self.todo.title
        }
    }
}

#[derive(Clone)]
pub struct TagInserter {
    fm: Arc<FileManager>,
    config: Arc<Config>,
    max_inserter_concurrency: usize,
}

impl TagInserter {
    make_spawn!{
        FileId,
        #[inline]
        pub fn new(
            fm: Arc<FileManager>,
            config: Arc<Config>,
            max_inserter_concurrency: usize
        ) -> Self {
            Self { fm, config, max_inserter_concurrency }
        }
    }

    pub async fn run(&self, mut tag_rx: UnboundedReceiver<FileId>) {
        let sem = Arc::new(Semaphore::new(self.max_inserter_concurrency));

        while let Some(file_id) = tag_rx.recv().await {
            let permit = sem.clone().acquire_owned().await.unwrap();
            let inserter = self.clone();

            tokio::task::spawn_blocking(move || {
                if let Err(err) = inserter.insert_tags(file_id) {
                    eprintln!{
                        "[tag] failed to insert tags for file {file_id:?}: {err:#}"
                    }
                }

                drop(permit);
            });
        }
    }

    fn insert_tags(&self, file_id: FileId) -> io::Result<()> {
        let mut insertions = mem::take(
            &mut self.fm.get_file_unchecked_mut(file_id).tags
        );

        if insertions.is_empty() {
            return Ok(())
        }

        // sort ascending so that all prior inserts were at <= current offset
        insertions.sort_by(|a, b| a.todo.tag_insertion_offset.cmp(&b.todo.tag_insertion_offset));

        let insertions = insertions.into_iter().map(|t| {
            (t.to_string(), t)
        }).collect::<Vec<_>>();

        let total_insert_len = insertions
            .iter()
            .map(|(s, _)| s.len())
            .sum::<usize>();

        let orig_len = self.fm.get_file_unchecked(file_id).meta.len() as usize;
        let new_len = orig_len + total_insert_len;

        let file_path = self.fm.get_file_path_unchecked(file_id).to_owned();

        let mut mmap = self.fm.get_mmap_or_remmap_file_mut(file_id, new_len)?;

        let mut shift = 0;

        for (tag_str, tag) in insertions {
            let byte_offset = tag.todo.tag_insertion_offset;
            let insert_bytes = tag_str.as_bytes();
            let tag_len = insert_bytes.len();

            let actual_offset = byte_offset + shift;
            mmap.copy_within(
                actual_offset..orig_len + shift,
                actual_offset + tag_len,
            );

            mmap[actual_offset..actual_offset + tag_len]
                .copy_from_slice(insert_bytes);

            shift += tag_len;

            mmap.flush()?;

            let msg = tag.commit_msg();

            self.config.commit_changes(&file_path, &msg)?;
        }

        Ok(())
    }
}

