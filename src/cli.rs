use crate::mode::Mode;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(
    name = "stalkr",
    about = "stalkr: multi-threaded TODO reporter",
    version = "0.1.0",
    // subcommand_required = true,
    // arg_required_else_help = true,
    override_usage = "stalkr [SUBCOMMAND] [OPTIONS]"
)]
pub struct Cli {
    #[clap(short, long, default_value = ".", global = true)]
    pub directory: PathBuf,

    #[clap(long, requires = "repository", global = true)]
    pub owner: Option<String>,

    #[clap(long, requires = "owner", global = true)]
    pub repository: Option<String>,

    #[clap(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    const DEFAULT_REMOTE: &str = "origin";

    #[inline(always)]
    #[must_use] 
    pub fn remote(&self) -> &str {
        match &self.command {
            Some(Commands::Purge { remote, .. })  => remote,
            Some(Commands::Report { remote, .. }) => remote,
            _ => Self::DEFAULT_REMOTE
        }
    }

    #[inline(always)]
    #[must_use] 
    pub fn simulate(&self) -> bool {
        match &self.command {
            Some(Commands::Report { simulate, .. }) => *simulate,
            _ => false
        }
    }

    #[inline(always)]
    #[must_use] 
    pub const fn mode(&self) -> Mode {
        match &self.command {
            Some(Commands::List { .. })  => Mode::Listing,
            Some(Commands::Purge { .. }) => Mode::Purging,

            _ => Mode::Reporting
        }
    }
}

#[derive(Subcommand)]
#[clap(about = "Subcommands for managing TODOs")]
pub enum Commands {
    /// NOTE: Not implemented yet
    /// Lists all TODOs
    #[clap(about = "Lists TODO comments found in a directory recursively")]
    List {
        /// Show only unreported TODOs
        #[clap(long, conflicts_with = "reported")]
        unreported: bool,

        /// Show only reported TODOs
        #[clap(long, conflicts_with = "unreported")]
        reported: bool,
    },

    /// NOTE: Not implemented yet
    /// Reports all TODOs as GitHub issues
    #[clap(about = "Reports TODO comments as GitHub issues")]
    Report {
        /// Report all todo's
        #[clap(
            long,
            short = 'y',
            help = "*Not actually implemented yet*"
        )]
        yes: bool,

        #[clap(long, default_value = Cli::DEFAULT_REMOTE)]
        remote: String,

        #[clap(
            long,
            default_value = "false",
            help = "Don't actually report a TODO to an API and don't actually insert an issue tag"
        )]
        simulate: bool,
    },

    /// NOTE: Not implemented yet
    /// Removes all reported TODOs that refer to closed issues
    #[allow(unused)]
    #[clap(about = "Removes TODO comments linked to closed GitHub issues")]
    Purge {
        #[clap(long, default_value = Cli::DEFAULT_REMOTE)]
        remote: String,
    }
}
