// TODO(#23): Implement purging
// TODO(#4): Improve UX of TODO selection
// TODO(#2): Commit tag-insertion to the origin

#[cfg(not(feature = "no_mimalloc"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[macro_use]
mod util;

mod fm;
mod loc;
mod tag;
mod cli;
mod mode;
mod todo;
mod issue;
mod purge;
mod stalk;
mod config;
mod prompt;

use cli::Cli;
use mode::Mode;
use stalk::Stalkr;
use issue::Issuer;
use config::Config;
use fm::FileManager;
use tag::TagInserter;
use prompt::{Prompter, PrompterTx};

use clap::Parser;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config = match Config::new(cli) {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => panic!("[{e}]")
    };

    let num_cpus = thread::available_parallelism().unwrap().get();

    let (rayon_threads, max_http_concurrency) = util::balance_concurrency(num_cpus);

    rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_threads)
        .build_global()
        .expect("[could not build global rayon threadpool]");

    // ---------------------- worker channels ----------------------

    // stalkr workers  -> prompter thread
    let (prompter_tx, prompter_rx) = unbounded_channel();

    // prompter thread -> issue workers
    let (issue_tx   ,    issue_rx) = unbounded_channel();

    // issue workers   -> inserter workers
    let (inserter_tx, inserter_rx) = unbounded_channel();

    // ---------------------- todo's counts ----------------------

    // the count of found (unreported + reported) todo's
    let found_count = Arc::new(AtomicUsize::new(0));

    // the count of reported todo's
    let reported_count = Arc::new(AtomicUsize::new(0));

    // ---------------------- workers spawns ----------------------

    let fm = Arc::new(FileManager::default());

    let prompter_task = Prompter::spawn(
        fm.clone(),
        config.clone(),
        match config.mode {
            Mode::Purging   => PrompterTx::Inserter(inserter_tx.clone()),
            Mode::Reporting => PrompterTx::Issuer(issue_tx.clone()),
            Mode::Listing   => todo!()
        },
        prompter_rx
    );

    let stalkr_task = Stalkr::spawn(
        fm.clone(),
        config.clone(),
        issue_tx.clone(),
        prompter_tx.clone(),
        found_count.clone()
    );

    let issue_task = Issuer::spawn(
        prompter_tx.clone(),
        inserter_tx,
        config.clone(),
        fm.clone(),
        reported_count.clone(),
        max_http_concurrency,
        issue_rx
    );

    let inserter_task = TagInserter::spawn(
        fm.clone(),
        config.clone(),
        num_cpus.min(4),
        inserter_rx
    );

    stalkr_task.await.expect("[could not await parsing workers]");

    drop(issue_tx);
    issue_task.await.expect("[could not await issuing workers]");

    drop(prompter_tx);
    prompter_task.await.expect("[could not await prompting thread]");

    inserter_task.await.expect("[could not await tag inserting workers]");

    let found_count    = found_count.load(Ordering::SeqCst);
    let reported_count = reported_count.load(Ordering::SeqCst);

    if found_count == 0 {
        println!("[no todo's found]")
    } else {
        println!("[{reported_count}/{found_count}] todo's reported")
    }
}
