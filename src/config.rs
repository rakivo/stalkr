use crate::util;
use crate::cli::Cli;

use std::{fs, env};
use std::path::PathBuf;

pub struct Config {
    pub owner    : Box<str>,
    pub repo     : Box<str>,
    pub gh_token : Box<str>,
    pub cwd      : Box<PathBuf>
}

impl Config {
    pub fn new(cli: Cli) -> anyhow::Result::<Self> {
        let Ok(gh_token) = env::var(
            "STALKR_GITHUB_TOKEN"
        ) else {
            return Err(anyhow::anyhow!{
                "could not get STALKR_GITHUB_TOKEN env variable"
            })
        };

        let (owner, repo) = match Self::get_git_origin_url(
            cli.directory.to_owned()
        ).as_deref().and_then(util::parse_owner_repo) {
            Some(x) => x,
            None => return Err(anyhow::anyhow!{
                "could not detect Github owner/repo"
            })
        };

        let cwd = Box::new(cli.directory);

        let owner = util::string_into_boxed_str_norealloc(owner);
        let repo = util::string_into_boxed_str_norealloc(repo);
        let gh_token = util::string_into_boxed_str_norealloc(gh_token);

        Ok(Self { owner, repo, gh_token, cwd })
    }

    #[inline]
    pub fn get_issues_api_url(&self) -> String {
        let Self { owner, repo, .. } = self;
        format!{
            "https://api.github.com/repos/{owner}/{repo}/issues"
        }
    }

    pub fn get_git_origin_url(mut dir: PathBuf) -> Option<String> {
        loop {
            let config = dir.join(".git/config");

            if config.exists() {
                let contents = fs::read_to_string(config).ok()?;

                let mut in_origin = false;
                for line in contents.lines() {
                    let line = line.trim();
                    if line.starts_with("[remote \"") {
                        in_origin = line.contains("\"origin\"");
                    } else if in_origin && line.starts_with("url") {
                        return line.split('=')
                            .nth(1)
                            .map(|s| s.trim().to_owned())
                    }
                }

                break
            }

            // go up
            if !dir.pop() { break }
        }

        None
    }
}
