use crate::todo::Todo;
use crate::config::Config;
use crate::issue::{Issue, Issuer};

#[async_trait::async_trait]
pub trait Api: Send + Sync {
    fn get_api_token_env_var(&self) -> &str;
    fn get_api_token(&self) -> anyhow::Result<String>;

    fn get_project_url(&self, config: &Config) -> String;
    fn get_issues_api_url(&self, config: &Config) -> String;
    fn get_issue_api_url(&self, config: &Config, issue: &Issue) -> String;

    fn make_client(&self, config: &Config) -> surf::Result<surf::Client>;

    async fn post_issue(&self, issuer: &Issuer, todo: Todo);
    async fn check_if_issue_is_closed(&self, issuer: &Issuer, issue: &Issue) -> bool;
}
