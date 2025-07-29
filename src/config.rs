use crate::util;

use std::env;

pub struct Config {
    pub owner    : Box<str>,
    pub repo     : Box<str>,
    pub gh_token : Box<str>
}

impl Config {
    pub fn new() -> anyhow::Result::<Self> {
        let Ok(gh_token) = env::var(
            "STALKR_GITHUB_TOKEN"
        ) else {
            return Err(anyhow::anyhow!{
                "could not get STALKR_GITHUB_TOKEN env variable"
            })
        };

        let (owner, repo) = match util::get_git_origin_url()
            .as_deref()
            .and_then(util::parse_owner_repo)
        {
            Some(x) => x,
            None => return Err(anyhow::anyhow!{
                "could not detect Github owner/repo"
            })
        };

        let owner = util::string_into_boxed_str_norealloc(owner);
        let repo = util::string_into_boxed_str_norealloc(repo);
        let gh_token = util::string_into_boxed_str_norealloc(gh_token);

        Ok(Self { owner, repo, gh_token })
    }

    #[inline]
    pub fn get_issues_api_url(&self) -> String {
        let Self { owner, repo, .. } = self;
        format!{
            "https://api.github.com/repos/{owner}/{repo}/issues"
        }
    }
}
