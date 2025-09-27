// TODO(#32): Implement closing mode
// TODO(#37): Allow user to scroll todo's

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
use std::process::{exit, ExitCode};
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::Parser;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match Config::new(&cli) {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE
        }
    };

    let num_cpus = thread::available_parallelism()
        .expect("[couldn't get num cpus]")
        .get();

    let (
        rayon_threads,
        max_http_concurrency
    ) = stalkr::util::balance_concurrency(num_cpus);

    // threadpool for stalkr workers
    if rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_threads)
        .build_global()
        .is_err()
    {
        eprintln!("[could not build global rayon threadpool]");
        return ExitCode::FAILURE
    }

    // ---------------------- todo's counts ----------------------

    // the count of found (unreported + reported) todo's
    let found_count = Arc::new(AtomicUsize::new(0));

    // the count of reported todo's
    let processed_count = Arc::new(AtomicUsize::new(0));

    ctrlc::set_handler({
        let config          = config.clone();
        let found_count     = found_count.clone();
        let processed_count = processed_count.clone();
        move || {
            let found_count     = found_count.load(Ordering::Acquire);
            let processed_count = processed_count.load(Ordering::Acquire);
            println!();
            config.mode.print_finish_msg(found_count, processed_count);
            exit(0);
        }
    }).unwrap();

    let fm = Arc::new(FileManager::default());

    if config.mode == Mode::Listing {
        listing(
            fm,
            config,
            found_count,
            processed_count
        ).await
    } else {
        reporting_and_purging(
            fm,
            config,
            found_count,
            processed_count,
            num_cpus,
            max_http_concurrency
        ).await
    }

    ExitCode::SUCCESS
}

async fn reporting_and_purging(
    fm: Arc<FileManager>,
    config: Arc<Config>,
    found_count: Arc<AtomicUsize>,
    processed_count: Arc<AtomicUsize>,
    num_cpus: usize,
    max_http_concurrency: usize
) {
    // ---------------------- worker channels ----------------------

    // stalkr workers  -> prompter thread
    let (prompter_tx, prompter_rx) = unbounded_channel();

    // prompter thread -> issue workers
    let (issue_tx   ,    issue_rx) = unbounded_channel();

    // issue workers   -> inserter workers
    let (inserter_tx, inserter_rx) = unbounded_channel();

    // ---------------------- workers spawns ----------------------

    let prompter_task = Prompter::spawn(
        fm.clone(),
        config.clone(),
        match config.mode {
            Mode::Purging   => PrompterTx::Inserter(inserter_tx.clone()),
            Mode::Reporting => PrompterTx::Issuer(issue_tx.clone()),
            Mode::Listing   => unreachable!(),
        },
        processed_count.clone(),
        prompter_rx
    );

    let stalkr_task = Stalkr::spawn(
        fm.clone(),
        config.clone(),
        match config.mode {
            Mode::Purging   => StalkrTx::Issuer(issue_tx.clone()),
            Mode::Reporting => StalkrTx::Prompter(prompter_tx.clone()),
            Mode::Listing   => unreachable!(),
        },
        found_count.clone()
    );

    let issue_task = Issuer::spawn(
        match config.mode {
            Mode::Purging   => IssuerTx::Prompter(prompter_tx.clone()),
            Mode::Reporting => IssuerTx::Inserter(inserter_tx.clone()),
            Mode::Listing   => unreachable!()
        },
        config.clone(),
        fm.clone(),
        found_count.clone(),
        processed_count.clone(),
        max_http_concurrency,
        issue_rx
    );

    let inserter_task = TagInserter::spawn(
        fm,
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

    config.mode.print_finish_msg(found_count, processed_count);
}

async fn listing(
    fm: Arc<FileManager>,
    config: Arc<Config>,
    found_count: Arc<AtomicUsize>,
    processed_count: Arc<AtomicUsize>,
) {
    // stalkr workers -> prompter thread
    let (prompter_tx, prompter_rx) = unbounded_channel();

    let prompter_task = Prompter::spawn(
        fm.clone(),
        config.clone(),
        PrompterTx::Listing,
        processed_count.clone(),
        prompter_rx
    );

    let stalkr_task = Stalkr::spawn(
        fm,
        config.clone(),
        StalkrTx::Prompter(prompter_tx.clone()),
        found_count.clone()
    );

    drop(prompter_tx);

    let (stalkr_res, prompter_res) = tokio::join!(stalkr_task, prompter_task);
    stalkr_res.expect("[could not await parsing workers]");
    prompter_res.expect("[could not await prompter thread]");

    let found_count     = found_count.load(Ordering::Acquire);
    let processed_count = processed_count.load(Ordering::Acquire);

    config.mode.print_finish_msg(found_count, processed_count);
}
