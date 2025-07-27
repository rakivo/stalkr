// TODO(#1): Move IO (ask_yn) outside of rayon `par_bridge`
//   It doesn't seem to work consistently for some reason,
//   but neither do I want to use global IO mutex for `ask_yn`.
//   So, moving it outside of rayon and doing IO in the main thread
//   seems like the way.

use std::sync::Arc;
use std::{env, thread};
use std::sync::atomic::{AtomicUsize, Ordering};

mod fm;
mod loc;
mod tag;
mod todo;
mod util;
mod issue;
mod stalk;
mod search;

use fm::FileManager;
use search::SearchCtx;

use dir_rec::DirRec;
use rayon::prelude::*;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::main]
async fn main() {
    let token = env::var(
        "STALKR_GITHUB_TOKEN"
    ).expect("STALKR_GITHUB_TOKEN env var missing");

    let num_cpus = thread::available_parallelism().unwrap().get();

    let (rayon_threads, max_http_concurrency) = util::balance_concurrency(num_cpus);

    let rayon_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_threads)
        .build()
        .unwrap();

    let (issue_tx, issue_rx) = unbounded_channel();

    let fm = Arc::new(FileManager::default());

    let found_count = Arc::new(AtomicUsize::new(1));

    let scan_handle = tokio::task::spawn_blocking({
        let tx = issue_tx.clone();
        let fm = fm.clone();
        let found_count = found_count.clone();
        let search_ctx = SearchCtx::new(todo::TODO_REGEXP);
        move || rayon_pool.install(move || {
            DirRec::new(".")
                .filter_map(|e| search_ctx.filter(e))
                .par_bridge()
                .for_each(|e| {
                    _ = stalk::stalk(
                        e,
                        &search_ctx,
                        &found_count,
                        &tx,
                        &fm
                    );
                });
        })
    });

    let (tag_tx, tag_rx) = unbounded_channel();

    let issue_handle = tokio::spawn({
        issue::issue(
            issue_rx,
            tag_tx,
            token,
            max_http_concurrency,
            fm.clone(),
        )
    });

    let num_inserters = num_cpus.min(4);

    let inserter_task = tokio::spawn(
        tag::poll_ready_files(
            tag_rx,
            fm.clone(),
            num_inserters,
        )
    );

    _ = scan_handle.await;

    drop(issue_tx);

    let reported_count = issue_handle.await.unwrap();

    inserter_task.await.unwrap();

    let found_count = found_count.load(Ordering::SeqCst);

    if found_count == 0 {
        println!("[no todo's found]")
    } else {
        println!("[{reported_count}/{found_count}] todo's reported")
    }
}
