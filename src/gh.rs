use crate::tag::Tag;
use crate::api::Api;
use crate::todo::Todo;
use crate::config::Config;
use crate::issue::{Issue, Issuer};

use std::env;
use std::sync::atomic::Ordering;

use surf::StatusCode;
use serde_json::Value;

pub struct GithubApi;

#[async_trait::async_trait]
impl Api for GithubApi {
    #[inline(always)]
    fn get_api_token_env_var(&self) -> &'static str {
        "STALKR_GITHUB_TOKEN"
    }

    #[inline(always)]
    fn get_api_token(&self) -> anyhow::Result<String> {
        env::var(self.get_api_token_env_var()).map_err(Into::into)
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
    fn make_client(&self, _config: &Config) -> surf::Result<surf::Client> {
        Ok(surf::Client::new())
    }

    async fn post_issue(&self, issuer: &Issuer, todo: Todo) {
        let body = todo.as_json_value();

        let rq = match issuer.rq_client
            .post(&*issuer.issues_api_url)
            .header("Authorization", format!("token {}", issuer.config.token()))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "stalkr-todo-bot")
            .body_json(&body)
        {
            Ok(rq) => rq,
            Err(e) => {
                eprintln!("[error creating request: {e}]");
                return
            }
        };

        match rq.await {
            Ok(mut r) if r.status().is_success() => {
                match r.body_json::<Value>().await {
                    Ok(json) => {
                        let issue_number = json
                            .get("number")
                            .and_then(serde_json::Value::as_u64)
                            .ok_or_else(|| anyhow::anyhow!("could not parse issue id"));

                        match issue_number {
                            Ok(issue_number) => {
                                let file_id = todo.loc.file_id();
                                let tag = Tag { issue_number, todo };
                                issuer.fm.add_tag_to_file(file_id, tag);
                            }
                            Err(e) => eprintln!("[failed to parse JSON response: {e}]")
                        }
                    }
                    Err(e) => eprintln!("[failed to parse JSON response: {e}]")
                }
            }

            Ok(r) if matches!{
                r.status(),
                StatusCode::Forbidden | StatusCode::TooManyRequests
            } => eprintln!{
                "[presumably rate limit hit: HTTP {status}]",
                status = r.status()
            },

            Ok(mut r) => {
                let text = r.body_string().await.unwrap_or_default();
                eprintln!{
                    "[failed to create issue ({s}): {t}]",
                    s = r.status(),
                    t = text
                }
            },

            Err(e) => eprintln!("[network error creating issue: {e}]")
        }
    }

    async fn check_if_issue_is_closed(&self, issuer: &Issuer, issue: &Issue) -> bool {
        let url = issuer.config.api.get_issue_api_url(&issuer.config, issue);

        let request = issuer.rq_client
            .get(&url)
            .header("Authorization", format!("token {}", issuer.config.token()))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "stalkr-todo-bot");

        match request.send().await {
            Ok(mut r) if r.status().is_success() => {
                let Ok(json) = r.body_json::<Value>().await else {
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
                StatusCode::Forbidden | StatusCode::TooManyRequests
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
