use crate::util;
use crate::todo::Todo;
use crate::prompt::Prompt;
use crate::config::Config;
use crate::mode::ModeValue;
use crate::fm::FileManager;
use crate::purge::{Purge, Purges};
use crate::tag::{Tag, InserterValue};

use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicUsize};

use futures::{stream, StreamExt};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub struct Issue {
    pub issue_number: u64
}

pub type IssueValue = ModeValue;

#[derive(Clone)]
pub enum IssuerTx {
    Prompter(UnboundedSender<Prompt>),
    Inserter(UnboundedSender<InserterValue>),
}

#[derive(Clone)]
pub struct Issuer {
    pub issues_api_url: Arc<str>,
    pub issuer_tx: IssuerTx,
    pub found_count: Arc<AtomicUsize>,
    pub processed_count: Arc<AtomicUsize>,
    pub config: Arc<Config>,
    pub fm: Arc<FileManager>,
    pub max_http_concurrency: usize,
    pub rq_client: reqwest::Client
}

impl Issuer {
    const MAX_PATH_LEN  : usize = 40; // path + line number + dots should not exceed this
    const MAX_TOTAL_LEN : usize = Self::MAX_PATH_LEN + 13; // total length before "is closed.." starts

    make_spawn!{
        IssueValue,
        pub fn new(
            issuer_tx: IssuerTx,
            config: Arc<Config>,
            fm: Arc<FileManager>,
            found_count: Arc<AtomicUsize>,
            processed_count: Arc<AtomicUsize>,
            max_http_concurrency: usize,
        ) -> Self {
            let rq_client = config.api.make_client(&config)
                .expect("failed to build API client");

            let issues_api_url = Arc::from(config.api.get_issues_api_url(&config));

            Self {
                issuer_tx,
                issues_api_url,
                found_count,
                processed_count,
                config,
                fm,
                max_http_concurrency,
                rq_client
            }
        }
    }

    pub async fn run(&self, issue_rx: UnboundedReceiver<IssueValue>) {
        UnboundedReceiverStream::new(issue_rx).for_each_concurrent(self.max_http_concurrency, |mode_value| {
            let issuer = self.clone();
            async move {
                debug_assert!(!mode_value.is_empty());

                match (mode_value, &issuer.issuer_tx) {
                    (ModeValue::Reporting(todos), IssuerTx::Inserter(inserter_tx)) => {
                        let file_id = todos[0].loc.file_id();

                        stream::iter(todos.into_iter()).for_each_concurrent(4, |todo| {
                            let issuer = issuer.clone();
                            async move {
                                issuer.post_todo(todo).await
                            }
                        }).await;

                        inserter_tx
                            .send(InserterValue::Inserting(file_id))
                            .expect("[failed to send file id to inserting worker]");
                    }

                    (ModeValue::Purging(purges), IssuerTx::Prompter(prompter_tx)) => {
                        let closed = stream::iter(purges.purges.into_iter())
                            .map(|purge| {
                                let issuer = self.clone();
                                async move {
                                    let is_closed = issuer.check_if_purge_needed(&purge).await;
                                    (purge, is_closed)
                                }
                            })
                            .buffer_unordered(4)
                            .filter_map(|(purge, is_closed)| async move {
                                if is_closed { Some(purge) } else { None }
                            })
                            .collect::<Vec<_>>()
                            .await;

                        if closed.is_empty() {
                            return
                        }

                        self.found_count.fetch_add(closed.len(), Ordering::SeqCst);

                        let purges = Purges {
                            purges: closed,
                            file_id: purges.file_id
                        };

                        let mode_value = ModeValue::Purging(purges);

                        prompter_tx
                            .send(Prompt { mode_value })
                            .expect("[failed to send file id to inserting worker]");
                    }

                    _ => unreachable!("unreachable tx-value combination")
                }
            }
        }).await;
    }

    async fn check_if_purge_needed(&self, purge: &Purge) -> bool {
        if !self.config.found_closed_todo.load(Ordering::SeqCst) {
            let line_number = purge.tag.todo.loc.line_number();
            let file_path = self.fm.get_file_path_unchecked(purge.tag.todo.loc.file_id());
            let truncated_path = util::truncate_path(
                &file_path,
                line_number,
                Self::MAX_PATH_LEN
            );

            let path_with_line = format!("{truncated_path}:{line_number}");
            let path_dots_needed = Self::MAX_PATH_LEN.saturating_sub(path_with_line.len());
            let path_dots = ".".repeat(path_dots_needed);

            let issue_str = format!("(issue #{x})", x = purge.tag.issue_number);
            let issue_dots_needed = 15usize.saturating_sub(issue_str.len());
            let issue_dots = ".".repeat(issue_dots_needed);

            let prefix = format!("{path_with_line}{path_dots}{issue_str}{issue_dots}");

            // dots after issue to align "is closed.."
            let dots_after_issue = ".".repeat(Self::MAX_TOTAL_LEN.saturating_sub(prefix.len()));

            println!("[checking if TODO at {prefix}{dots_after_issue}is closed..]");
        }

        self.config.api.check_if_issue_is_closed(
            self,
            &Issue { issue_number: purge.tag.issue_number }
        ).await
    }

    async fn post_todo(&self, todo: Todo) {
        if self.config.simulate_reporting {
            // simulate network latency
            use tokio::time::{sleep, Duration};

            sleep(Duration::from_millis(150)).await;

            self.processed_count.fetch_add(1, Ordering::SeqCst);

            // fake issue number
            let issue_number = rand::random::<u64>() % 10_000;
            let file_id = todo.loc.file_id();
            let tag = Tag { todo, issue_number };
            self.fm.add_tag_to_file(file_id, tag);

            return
        }

        self.config.api.post_issue(self, todo).await
    }
}

