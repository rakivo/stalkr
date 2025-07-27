use crate::todo::Todo;
use crate::fm::FileManager;

use std::sync::Arc;
use std::error::Error;
use std::sync::atomic::{Ordering, AtomicUsize};

use serde_json::Value;
use futures::StreamExt;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::wrappers::UnboundedReceiverStream;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, AUTHORIZATION};

pub async fn issue(
    rx: UnboundedReceiver<Todo>,
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

    UnboundedReceiverStream::new(rx).for_each_concurrent(max_concurrency, |todo| {
        let fm = fm.clone();
        let url = url.clone();
        let rq_client = rq_client.clone();
        let reported_count = reported_count.clone();

        async move {
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
                        Ok(num) => {
                            println!("created issue with number {num}");
                            reported_count.fetch_add(1, Ordering::SeqCst);
                            crate::tag::insert_tag_mmap(
                                todo.src_file_id,
                                todo.todo_byte_offset,
                                &format!("(#{num})"),
                                &fm
                            ).unwrap();
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
    }).await;

    reported_count.load(Ordering::SeqCst)
}

