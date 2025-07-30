use crate::tag::Tag;
use crate::config::Config;
use crate::todo::{Todo, Todos};
use crate::fm::{FileId, FileManager};

use std::sync::Arc;
use std::error::Error;
use std::sync::atomic::{Ordering, AtomicUsize};

use serde_json::Value;
use futures::{stream, StreamExt};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

#[derive(Clone)]
pub struct Issuer {
    pub issues_api_url: Arc<str>,
    pub inserter_tx: UnboundedSender<FileId>,
    pub reported_count: Arc<AtomicUsize>,
    #[allow(unused)]
    pub config: Arc<Config>,
    pub fm: Arc<FileManager>,
    pub max_http_concurrency: usize,
    pub rq_client: reqwest::Client
}

impl Issuer {
    make_spawn!{
        Todos,
        pub fn new(
            inserter_tx: UnboundedSender<FileId>,
            config: Arc<Config>,
            fm: Arc<FileManager>,
            reported_count: Arc<AtomicUsize>,
            max_http_concurrency: usize,
        ) -> Self {
            let headers = HeaderMap::from_iter([
                (
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!{
                        "token {token}",
                        token = config.gh_token
                    }).unwrap()
                ),
            ]);

            let rq_client = reqwest::Client::builder()
                .user_agent("stalkr-todo-bot")
                .default_headers(headers)
                .build()
                .unwrap();

            let issues_api_url = Arc::from(config.get_issues_api_url());

            Self {
                issues_api_url,
                inserter_tx,
                reported_count,
                config,
                fm,
                max_http_concurrency,
                rq_client
            }
        }
    }

    pub async fn run(&self, issue_rx: UnboundedReceiver<Todos>) {
        UnboundedReceiverStream::new(issue_rx).for_each_concurrent(self.max_http_concurrency, |todos| {
            let issuer = self.clone();
            async move {
                debug_assert!(!todos.is_empty());

                let file_id = todos[0].loc.file_id();

                stream::iter(todos.into_iter()).for_each_concurrent(4, |todo| {
                    let issuer = issuer.clone();
                    async move {
                        issuer.issue_todo(todo).await;
                    }
                }).await;

                self.inserter_tx
                    .send(file_id)
                    .expect("[failed to send file id to inserting worker]");
            }
        }).await;
    }

    async fn issue_todo(&self, todo: Todo) {
        let body = todo.as_json_value();

        let rs = self.rq_client.post(
            &*self.issues_api_url
        ).json(&body).send().await;

        match rs {
            Ok(r) if r.status().is_success() => {
                let issue_number = r
                    .json::<Value>()
                    .await
                    .map_err(|e| e.into())
                    .and_then(|j| {
                        j.get("number").and_then(|v| v.as_u64()).ok_or_else(|| {
                            anyhow::anyhow!("could not parse issue id")
                        })
                    });

                match issue_number {
                    Ok(issue_number) => {
                        self.reported_count.fetch_add(1, Ordering::SeqCst);

                        let tag = Tag {
                            issue_number,
                            byte_offset: todo.todo_global_offset as _,
                        };

                        self.fm.add_tag_to_file(todo.loc.file_id(), tag);
                    }
                    Err(e) => eprintln!("[failed to parse JSON response: {e}]")
                }
            }

            Ok(r) => eprintln!{
                "[failed to create issue ({s}): {t:?}]",
                s = r.status(),
                t = r.text().await
            },

            Err(e) => {
                eprintln!("[network error creating issue: {e}]");
                let mut src = e.source();
                while let Some(s) = src {
                    eprintln!("  caused by: {s}");
                    src = s.source();
                }
            }
        }
    }
}
