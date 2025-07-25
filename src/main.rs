use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

mod loc;
mod todo;
mod util;
mod issue;
mod stalk;
mod search;

use search::SearchCtx;

use dir_rec::DirRec;
use rayon::prelude::*;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::main]
async fn main() {
    let token = env::var(
        "STALKR_GITHUB_TOKEN"
    ).expect("STALKR_GITHUB_TOKEN env var missing");

    let (tx, rx) = unbounded_channel();

    let found_count = Arc::new(AtomicUsize::new(0));

    let scan_handle = tokio::task::spawn_blocking({
        let tx = tx.clone();
        let found_count = found_count.clone();
        let search_ctx = SearchCtx::new(todo::TODO_REGEXP).unwrap();
        move || {
            DirRec::new(".")
                .into_iter()
                .filter_map(|e| search_ctx.filter(e))
                .par_bridge()
                .for_each(|e| {
                    _ = stalk::stalk(e, &search_ctx, &found_count, &tx);
                });
        }
    });

    let issue_handle = tokio::spawn(issue::issue_worker(
        rx,
        token,
        issue::MAX_CONCURRENCY
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
