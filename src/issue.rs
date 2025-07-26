use crate::todo::Todo;

use std::sync::Arc;
use std::error::Error;
use std::sync::atomic::{Ordering, AtomicUsize};

use futures::StreamExt;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::wrappers::UnboundedReceiverStream;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, AUTHORIZATION};

pub const MAX_CONCURRENCY: usize = 4;

pub async fn issue(
    rx: UnboundedReceiver<Todo>,
    token: String,
    max_concurrency: usize,
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

    let stream = UnboundedReceiverStream::new(rx).map(|todo| {
        let url = url.clone();
        let client = rq_client.clone();
        let reported_count = reported_count.clone();

        async move {
            let body = serde_json::json!({
                "title": todo.title,
                "body": todo.description.map(|ls| ls.lines.join("\n"))
            });

            let resp = client.post(&*url).json(&body).send().await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    reported_count.fetch_add(1, Ordering::SeqCst);
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
    });

    stream.buffer_unordered(max_concurrency).for_each(|_| async {}).await;

    reported_count.load(Ordering::SeqCst)
}

