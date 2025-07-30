use crate::util;
use crate::cli::Cli;

use std::{fs, io, env};
use std::path::PathBuf;
use std::process::Command;

#[derive(Eq, Copy, Clone, Debug, PartialEq)]
pub enum Mode {
    Purging,
    Listing,
    Reporting
}

pub struct Config {
    pub owner    : Box<str>,
    pub repo     : Box<str>,
    pub gh_token : Box<str>,
    pub cwd      : Box<PathBuf>,
    pub mode     : Mode
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

        let remote = cli.remote();

        let (owner, repo) = match Self::get_git_origin_url(
            cli.directory.to_owned(),
            &remote
        ).as_deref().and_then(util::parse_owner_repo) {
            Some(x) => x,
            None => return Err(anyhow::anyhow!{
                "could not detect Github owner/repo"
            })
        };

        let cwd = Box::new(cli.directory.to_owned());

        let mode = cli.mode();

        let owner    = util::string_into_boxed_str_norealloc(owner);
        let repo     = util::string_into_boxed_str_norealloc(repo);
        let gh_token = util::string_into_boxed_str_norealloc(gh_token);

        Ok(Self { owner, repo, gh_token, cwd, mode })
    }

    #[inline(always)]
    pub fn get_project_url(&self) -> String {
        let Self { owner, repo, .. } = self;
        format!{
            "https://github.com/{owner}/{repo}"
        }
    }

    #[inline(always)]
    pub fn get_issues_api_url(&self) -> String {
        let Self { owner, repo, .. } = self;
        format!{
            "https://api.github.com/repos/{owner}/{repo}/issues"
        }
    }

    #[inline(always)]
    pub fn get_issue_api_url(&self, issue_number: u64) -> String {
        let Self { owner, repo, .. } = self;
        format!{
            "https://api.github.com/repos/{owner}/{repo}/issues/{issue_number}"
        }
    }

    pub fn get_git_origin_url(mut dir: PathBuf, remote: &str) -> Option<String> {
        loop {
            let config = dir.join(".git/config");

            if config.exists() {
                let contents = fs::read_to_string(config).ok()?;

                let mut in_origin = false;
                for line in contents.lines() {
                    let line = line.trim();
                    if line.starts_with("[remote \"") {
                        in_origin = line.contains(&format!{
                            "\"{remote}\""
                        })
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

    pub fn commit_changes(&self, path: &str, msg: &str) -> io::Result<()> {
        let status = Command::new("git")
            .arg("add")
            .arg(path)
            .status()?;

        if !status.success() {
            panic!("git add failed");
        }

        let status = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(msg)
            .status()?;

        if !status.success() {
            panic!("git commit failed");
        }

        Ok(())
    }
}
