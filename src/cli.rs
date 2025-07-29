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

#[derive(Subcommand)]
#[clap(about = "Subcommands for managing TODOs")]
pub enum Commands {
    /// NOTE: Not implemented yet
    /// Lists all TODOs in a directory recursively
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
        /// Auto-confirm actions
        #[clap(long, short = 'y')]
        yes: bool,

        /// Which remote to commit issues to
        #[clap(long)]
        remote: bool,
    },
    /// NOTE: Not implemented yet
    /// Removes all reported TODOs that refer to closed issues
    #[allow(unused)]
    #[clap(about = "Removes TODO comments linked to closed GitHub issues")]
    Purge {
        /// Perform operation remotely
        #[clap(long)]
        remote: bool,
    }
}
