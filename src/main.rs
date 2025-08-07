// TODO(#32): Implement closing mode
// TODO(#25): Support for inline todo's
// TODO(#4): Improve UX of TODO selection

use stalkr::cli::Cli;
use stalkr::mode::Mode;
use stalkr::config::Config;
use stalkr::fm::FileManager;
use stalkr::tag::TagInserter;
use stalkr::stalk::{Stalkr, StalkrTx};
use stalkr::issue::{Issuer, IssuerTx};
use stalkr::prompt::{Prompter, PrompterTx};

use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

    let (rayon_threads, max_http_concurrency) = stalkr::util::balance_concurrency(num_cpus);

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
    let processed_count = Arc::new(AtomicUsize::new(0));

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
        match config.mode {
            Mode::Purging   => StalkrTx::Issuer(issue_tx.clone()),
            Mode::Reporting => StalkrTx::Prompter(prompter_tx.clone()),
            Mode::Listing   => todo!()
        },
        found_count.clone()
    );

    let issue_task = Issuer::spawn(
        match config.mode {
            Mode::Purging   => IssuerTx::Prompter(prompter_tx.clone()),
            Mode::Reporting => IssuerTx::Inserter(inserter_tx.clone()),
            Mode::Listing   => todo!()
        },
        config.clone(),
        fm.clone(),
        processed_count.clone(),
        max_http_concurrency,
        issue_rx
    );

    let inserter_task = TagInserter::spawn(
        fm.clone(),
        config.clone(),
        processed_count.clone(),
        num_cpus.min(4),
        inserter_rx
    );

    // ---------------------- drop all senders ----------------------
    // drop all senders so receivers see EOF as soon as possible.
    drop(issue_tx);
    drop(prompter_tx);
    drop(inserter_tx);

    // ---------------------- await all tasks in parallel ----------------------
    let (stalkr_res, issue_res, prompter_res, inserter_res) = tokio::join!{
        stalkr_task,
        issue_task,
        prompter_task,
        inserter_task
    };

    stalkr_res.expect("[could not await parsing workers]");
    issue_res.expect("[could not await issuing workers]");
    prompter_res.expect("[could not await prompting thread]");
    inserter_res.expect("[could not await tag inserting workers]");

    let found_count     = found_count.load(Ordering::Acquire);
    let processed_count = processed_count.load(Ordering::Acquire);

    if found_count == 0 {
        println!("[no todo's found]")
    } else {
        println!{
            "[{processed_count}/{found_count}] todo's {what}",
            what = match config.mode {
                Mode::Purging   => "purged",
                Mode::Reporting => "reported",
                Mode::Listing   => "listed"
            }
        }
    }
}
