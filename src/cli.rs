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
    #[clap(short, long, default_value = ".")]
    pub directory: PathBuf,

    #[clap(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    const DEFAULT_REMOTE: &str = "origin";

    #[inline(always)]
    pub fn remote(&self) -> &str {
        match &self.command {
            Some(Commands::Purge { remote, .. })  => remote,
            Some(Commands::Report { remote, .. }) => remote,
            _ => Self::DEFAULT_REMOTE
        }
    }

    #[inline(always)]
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
        #[clap(long, short = 'y')]
        yes: bool,

        #[clap(long, default_value = Cli::DEFAULT_REMOTE)]
        remote: String,
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
