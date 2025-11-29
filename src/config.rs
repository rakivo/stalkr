use crate::util;
use crate::cli::Cli;
use crate::api::Api;
use crate::mode::Mode;
use crate::git::GitLocker;

use std::sync::Arc;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

pub struct Config {
    pub owner : Box<str>,
    pub repo  : Box<str>,
    pub token : Option<Box<str>>,
    pub cwd   : Box<PathBuf>,

    pub mode: Mode,

    pub api: Box<dyn Api>,

    pub git_locker: Arc<GitLocker>,

    pub simulate_reporting: bool,

    pub found_closed_todo: AtomicBool
}

impl Config {
    pub fn new(cli: &Cli) -> anyhow::Result::<Self> {
        let api = Box::new(crate::gh::GithubApi);

        let token = if cli.mode() == Mode::Listing {
            None
        } else {
            let Ok(token) = api.get_api_token() else {
                return Err(anyhow::anyhow!{
                    concat!{
                        "couldn't get {token} env variable\n",
                        "note: if you just want to list the todos, do: `stalkr list`",
                    },
                    token = api.get_api_token_env_var()
                })
            };
            Some(token)
        };

        let remote = cli.remote();

        let (owner, repo) = if let (Some(owner), Some(repo)) = (
            &cli.owner, &cli.repository
        ) {
            (owner.to_owned(), repo.to_owned())
        } else {
            match util::get_git_origin_url(
                cli.directory.clone(),
                remote
            ).as_deref().and_then(util::parse_owner_repo) {
                Some(x) => x,
                None => return Err(anyhow::anyhow!{
                    "couldn't detect Github owner/repo"
                })
            }
        };

        let cwd = Box::new(cli.directory.clone());

        let mode = cli.mode();

        let owner = util::string_into_boxed_str_norealloc(owner);
        let repo  = util::string_into_boxed_str_norealloc(repo);
        let token = token.map(util::string_into_boxed_str_norealloc);

        let simulate_reporting = cli.simulate();

        let found_closed_todo = AtomicBool::new(false);

        let git_locker = Arc::new(GitLocker::new());

        Ok(Self {
            owner,
            repo,
            token,
            cwd,
            mode,
            api,
            git_locker,
            simulate_reporting,
            found_closed_todo,
        })
    }

    #[track_caller]
    #[inline(always)]
    pub fn token(&self) -> &str {
        self.token.as_ref().unwrap()
    }
}
