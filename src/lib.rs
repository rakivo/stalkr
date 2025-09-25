#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::cargo,
    dead_code
)]
#![allow(
    clippy::collapsible_if,
    clippy::items_after_statements,
    clippy::struct_field_names,
    clippy::inline_always,
    clippy::redundant_field_names,
    clippy::multiple_crate_versions,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::used_underscore_binding,
    clippy::nonstandard_macro_braces,
    clippy::used_underscore_items,
    clippy::enum_glob_use,
    clippy::match_same_arms,
    clippy::too_many_lines,
    clippy::unnested_or_patterns,
    clippy::blocks_in_conditions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
)]

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[macro_use]
pub mod util;

pub mod gh;
pub mod fm;
pub mod git;
pub mod loc;
pub mod tag;
pub mod cli;
pub mod api;
pub mod mode;
pub mod todo;
pub mod issue;
pub mod purge;
pub mod stalk;
pub mod config;
pub mod prompt;
pub mod comment;
