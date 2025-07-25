use crate::todo::Todo;

use std::sync::Arc;
use std::error::Error;
use std::sync::atomic::{Ordering, AtomicUsize};

use tokio::task::JoinSet;
use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedReceiver;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, AUTHORIZATION};

pub const MAX_CONCURRENCY: usize = 4;

pub async fn issue_worker(
    mut rx: UnboundedReceiver<Todo>,
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

    // semaphore to cap concurrent GitHub requests (to avoid ratelimit storms)
    let sem = Arc::new(Semaphore::new(max_concurrency));

    let reported_count = Arc::new(AtomicUsize::new(0));

    let mut join_set = JoinSet::new();

    while let Some(todo) = rx.recv().await {
        let url = url.clone();
        let sem = sem.clone();
        let client = rq_client.clone();
        let reported_count = reported_count.clone();

        join_set.spawn(async move {
            let _p = sem.acquire().await.unwrap();

            let body = serde_json::json!({
                "title": todo.title,
                "body": format!{
                    "TODO at `{loc}`",
                    loc = todo.loc
                }
            });

            let resp = client.post(&*url)
                .json(&body)
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    reported_count.fetch_add(1, Ordering::SeqCst);
                }
                Ok(r) => {
                    eprintln!();
                    eprintln!(
                        "[failed to create issue ({s}): {t:?}]",
                        s = r.status(),
                        t = r.text().await
                    );
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
        });
    }

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res {
            eprintln!("[spawned task error: {e}]");
        }
    }

    reported_count.load(Ordering::SeqCst)
}

