use crate::tag::Tag;
use crate::api::Api;
use crate::todo::Todo;
use crate::config::Config;
use crate::issue::{Issue, Issuer};

use std::env;
use std::error::Error;
use std::time::Duration;
use std::sync::atomic::Ordering;

use serde_json::Value;
use reqwest::StatusCode;

pub struct GithubApi;

#[async_trait::async_trait]
impl Api for GithubApi {
    #[inline(always)]
    fn get_api_token_env_var(&self) -> &str {
        "STALKR_GITHUB_TOKEN"
    }

    #[inline(always)]
    fn get_api_token(&self) -> anyhow::Result<String> {
        env::var(self.get_api_token_env_var()).map_err(|e| e.into())
    }

    #[inline(always)]
    fn get_project_url(&self, config: &Config) -> String {
        let Config { owner, repo, .. } = config;
        format!{
            "https://github.com/{owner}/{repo}"
        }
    }

    #[inline(always)]
    fn get_issues_api_url(&self, config: &Config) -> String {
        let Config { owner, repo, .. } = config;
        format!{
            "https://api.github.com/repos/{owner}/{repo}/issues"
        }
    }

    #[inline(always)]
    fn get_issue_api_url(&self, config: &Config, issue: &Issue) -> String {
        let Config { owner, repo, .. } = config;
        let issue_number = issue.issue_number;
        format!{
            "https://api.github.com/repos/{owner}/{repo}/issues/{issue_number}"
        }
    }

    #[inline]
    fn make_client(&self, config: &Config) -> reqwest::Result<reqwest::Client> {
        use reqwest::Client;
        use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};

        Client::builder()
            .pool_max_idle_per_host(8)
            .user_agent("stalkr-todo-bot")
            .pool_idle_timeout(Duration::from_secs(90))
            .default_headers(HeaderMap::from_iter([
                (
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!{
                        "token {token}",
                        token = config.token
                    }).unwrap()
                ),
                (
                    ACCEPT,
                    "application/vnd.github.v3+json".parse().unwrap()
                )
            ])).build()
    }

    async fn post_issue(&self, issuer: &Issuer, todo: Todo) {
        let body = todo.as_json_value();

        let rs = issuer.rq_client
            .post(&*issuer.issues_api_url)
            .json(&body)
            .send()
            .await;

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
                        let file_id = todo.loc.file_id();
                        let tag = Tag { todo, issue_number };
                        issuer.fm.add_tag_to_file(file_id, tag);
                    }
                    Err(e) => eprintln!("[failed to parse JSON response: {e}]")
                }
            }

            Ok(r) if matches!{
                r.status(),
                StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
            } => eprintln!{
                "[presumably rate limit hit: HTTP {status}]",
                status = r.status()
            },

            Ok(r) => eprintln!{
                "[failed to create issue ({s}): {t}]",
                s = r.status(),
                t = r.text().await.unwrap_or_default()
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

    async fn check_if_issue_is_closed(&self, issuer: &Issuer, issue: &Issue) -> bool {
        let url = issuer.config.api.get_issue_api_url(&issuer.config, issue);

        match issuer.rq_client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                let Ok(json) = r.json::<Value>().await else {
                    return false
                };

                let Ok(state) = json.get("state").and_then(|s| s.as_str()).ok_or_else(|| {
                    anyhow::anyhow!("could not parse issue state")
                }) else {
                    return false
                };

                if state == "closed" {
                    issuer.config.found_closed_todo.store(true, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            }

            Ok(r) if matches!{
                r.status(),
                StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
            } => {
                // TODO(#27): A mechanism to stop execution
                eprintln!{
                    "[presumably rate limit hit: HTTP {status}]",
                    status = r.status()
                }; false
            }

            _ => false
        }
    }
}
