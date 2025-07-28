// TODO(#4): Improve UX of TODO selection
// TODO(#5): Unhard-code owner/repo
// TODO(#3): Auto-detect owner and repo
// TODO(#2): Commit tag-insertion to the origin

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
mod prompt;

use fm::FileManager;
use prompt::Prompter;
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

    // stalkr workers  -> prompter thread
    let (prompter_tx, prompter_rx) = unbounded_channel();

    // prompter thread -> issue workers
    let (issue_tx   ,    issue_rx) = unbounded_channel();

    // issue workers   -> inserter workers
    let (inserter_tx, inserter_rx) = unbounded_channel();

    // the count of found (unreported + reported) todo's
    let found_count = Arc::new(AtomicUsize::new(1));

    let fm = Arc::new(FileManager::default());

    let mut prompter = Prompter {
        fm: fm.clone(),
        prompter_rx,
        issue_tx: issue_tx.clone(),
    };

    let prompter_task = tokio::task::spawn(async move {
        prompter.prompt_loop().await
    });

    let stalkr_task = tokio::task::spawn_blocking({
        let fm = fm.clone();
        let found_count = found_count.clone();
        let prompter_tx = prompter_tx.clone();
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
                        &prompter_tx,
                        &fm
                    );
                });
        })
    });

    let issue_task = tokio::spawn({
        issue::issue(
            issue_rx,
            inserter_tx,
            token,
            max_http_concurrency,
            fm.clone(),
        )
    });

    let inserter_task = tokio::spawn(
        tag::poll_ready_files(
            inserter_rx,
            fm.clone(),
            num_cpus.min(4)
        )
    );

    _ = stalkr_task.await;

    drop(issue_tx);
    drop(prompter_tx);

    prompter_task.await.unwrap();

    let reported_count = issue_task.await.unwrap();

    inserter_task.await.unwrap();

    let found_count = found_count.load(Ordering::SeqCst);

    if found_count == 0 {
        println!("[no todo's found]")
    } else {
        println!("[{reported_count}/{found_count}] todo's reported")
    }
}
