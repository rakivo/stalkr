use std::sync::Arc;
use std::{env, thread};
use std::sync::atomic::{AtomicUsize, Ordering};

mod fm;
mod loc;
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

    let cpu_count = thread::available_parallelism().unwrap().get();

    let (rayon_threads, max_http_concurrency) = util::balance_concurrency(cpu_count);

    let rayon_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_threads)
        .build()
        .unwrap();

    let (tx, rx) = unbounded_channel();

    let fm = FileManager::default();

    let found_count = Arc::new(AtomicUsize::new(0));

    let scan_handle = tokio::task::spawn_blocking({
        let tx = tx.clone();
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

    let issue_handle = tokio::spawn(issue::issue(
        rx,
        token,
        max_http_concurrency
    ));

    _ = scan_handle.await;

    drop(tx);

    let found_count = found_count.load(Ordering::SeqCst);

    let reported_count = issue_handle.await.unwrap();

    if found_count == 0 {
        println!("[no todo's found]")
    } else {
        println!("[{reported_count}/{found_count}] todo's reported")
    }
}
