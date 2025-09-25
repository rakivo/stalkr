use crate::todo::Todo;
use crate::purge::Purges;
use crate::config::Config;
use crate::fm::{FileId, FileManager};

use std::{mem, fmt};
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicUsize};

use futures::StreamExt;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub enum InserterValue {
    Inserting(FileId),
    Purging(Purges)
}

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
    #[must_use]
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
    processed_count: Arc<AtomicUsize>,
    max_inserter_concurrency: usize,
}

impl TagInserter {
    make_spawn!{
        InserterValue,
        #[inline]
        pub fn new(
            fm: Arc<FileManager>,
            config: Arc<Config>,
            processed_count: Arc<AtomicUsize>,
            max_inserter_concurrency: usize
        ) -> Self {
            Self { fm, config, processed_count, max_inserter_concurrency }
        }
    }

    pub async fn run(&self, inserter_rx: UnboundedReceiver<InserterValue>) {
        let stream = UnboundedReceiverStream::new(inserter_rx);

        stream.for_each_concurrent(self.max_inserter_concurrency, |inserter_value| {
            let inserter = self.clone();

            async move {
                tokio::task::spawn_blocking(move || {
                    match inserter_value {
                        InserterValue::Inserting(file_id) => {
                            if let Err(err) = inserter.insert_tags(file_id) {
                                eprintln!{
                                    "[tag] failed to insert tagʼs for file {file_id:?}: {err:#}"
                                }
                            }
                        }

                        InserterValue::Purging(purges) => {
                            let file_id = purges.file_id;

                            if let Err(err) = purges.apply(
                                &inserter.processed_count,
                                &inserter.config,
                                &inserter.fm
                            ) {
                                eprintln!{
                                    "[tag] failed to purge todoʼs for file {file_id:?}: {err:#}"
                                }
                            }
                        }
                    }
                }).await.unwrap();
            }
        }).await;
    }

    fn insert_tags(&self, file_id: FileId) -> anyhow::Result<()> {
        if self.config.simulate_reporting { return Ok(()) }

        let mut insertions = mem::take(
            &mut self.fm.get_file_unchecked_mut(file_id).tags
        );

        if insertions.is_empty() { return Ok(()) }

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

            mmap.flush()?;

            let msg = tag.commit_msg();
            self.config.git_locker.commit_changes(&file_path, &msg)?;

            self.processed_count.fetch_add(1, Ordering::SeqCst);

            shift += tag_len;
        }

        Ok(())
    }
}

