#[cfg(not(feature = "no_mimalloc"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[macro_use]
pub mod util;

pub mod fm;
pub mod loc;
pub mod tag;
pub mod cli;
pub mod mode;
pub mod todo;
pub mod issue;
pub mod purge;
pub mod stalk;
pub mod config;
pub mod prompt;
