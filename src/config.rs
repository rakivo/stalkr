use crate::util;
use crate::cli::Cli;
use crate::api::Api;
use crate::mode::Mode;
use crate::git::GitLocker;

use std::fs;
use std::sync::Arc;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

pub struct Config {
    pub owner : Box<str>,
    pub repo  : Box<str>,
    pub token : Box<str>,
    pub cwd   : Box<PathBuf>,

    pub mode: Mode,

    pub api: Box<dyn Api>,

    pub git_locker: Arc<GitLocker>,

    pub simulate_reporting: bool,

    pub found_closed_todo: AtomicBool
}

impl Config {
    pub fn new(cli: Cli) -> anyhow::Result::<Self> {
        let api = Box::new(crate::gh::GithubApi);

        let Ok(token) = api.get_api_token() else {
            return Err(anyhow::anyhow!{
                "could not get {token} env variable",
                token = api.get_api_token_env_var()
            })
        };

        let remote = cli.remote();

        let (owner, repo) = if let (Some(owner), Some(repo)) = (
            &cli.owner, &cli.repository
        ) {
            (owner.to_owned(), repo.to_owned())
        } else {
            match Self::get_git_origin_url(
                cli.directory.to_owned(),
                remote
            ).as_deref().and_then(util::parse_owner_repo) {
                Some(x) => x,
                None => return Err(anyhow::anyhow!{
                    "could not detect Github owner/repo"
                })
            }
        };

        let cwd = Box::new(cli.directory.to_owned());

        let mode = cli.mode();

        let owner = util::string_into_boxed_str_norealloc(owner);
        let repo  = util::string_into_boxed_str_norealloc(repo);
        let token = util::string_into_boxed_str_norealloc(token);

        let simulate_reporting = cli.simulate();

        let found_closed_todo = AtomicBool::new(false);

        let git_locker = Arc::new(GitLocker::new());

        Ok(Self {
            api,
            owner,
            repo,
            token,
            cwd,
            mode,
            found_closed_todo,
            simulate_reporting,
            git_locker,
        })
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
}
