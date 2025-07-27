use crate::tag::Tag;
use crate::todo::{Todo, Todos};
use crate::fm::{FileId, FileManager};

use std::sync::Arc;
use std::error::Error;
use std::sync::atomic::{Ordering, AtomicUsize};

use serde_json::Value;
use futures::{stream, StreamExt};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, AUTHORIZATION};

async fn issue_todo(
    todo: Todo,
    url: Arc<str>,
    rq_client: reqwest::Client,
    reported_count: Arc<AtomicUsize>,
    fm: Arc<FileManager>
) {
    let body = serde_json::json!({
        "title": todo.title,
        "body": todo.description.map(|ls| ls.lines.join("\n"))
    });

    let resp = rq_client.post(&*url).json(&body).send().await;

    match resp {
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
                    reported_count.fetch_add(1, Ordering::SeqCst);

                    let tag = Tag {
                        issue_number,
                        byte_offset: todo.todo_byte_offset as _,
                    };

                    fm.add_tag_to_file(todo.src_file_id, tag);
                }
                Err(e) => eprintln!("[failed to parse JSON response: {e}]")
            }
        }
        Ok(r) => {
            eprintln!{
                "[failed to create issue ({s}): {t:?}]",
                s = r.status(),
                t = r.text().await
            };
        }
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

pub async fn issue(
    rx: UnboundedReceiver<Todos>,
    tag_tx: UnboundedSender<FileId>,
    token: String,
    max_concurrency: usize,
    fm: Arc<FileManager>
) -> usize {
    let (owner, repo) = ("rakivo", "stalkr-test");
    let url = Arc::<str>::from(format!{
        "https://api.github.com/repos/{owner}/{repo}/issues"
    });

    let headers = HeaderMap::from_iter([
        (
            AUTHORIZATION,
            HeaderValue::from_str(&format!("token {token}")).unwrap()
        ),

        (
            USER_AGENT,
            HeaderValue::from_static("stalkr-todo-bot")
        )
    ]);

    let rq_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    let reported_count = Arc::new(AtomicUsize::new(0));

    UnboundedReceiverStream::new(rx).for_each_concurrent(max_concurrency, |todos| {
        let fm = fm.clone();
        let url = url.clone();
        let tag_tx = tag_tx.clone();
        let rq_client = rq_client.clone();
        let reported_count = reported_count.clone();

        async move {
            let file_id = todos[0].src_file_id;

            stream::iter(todos.into_iter()).for_each_concurrent(4, |todo| {
                let url            = url.clone();
                let rq_client      = rq_client.clone();
                let reported_count = reported_count.clone();
                let fm             = fm.clone();

                async move {
                    issue_todo(todo, url, rq_client, reported_count, fm).await;
                }
            }).await;

            tag_tx.send(file_id).expect("[failed to send file id to inserting worker]");
        }
    }).await;

    reported_count.load(Ordering::SeqCst)
}

