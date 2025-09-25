use crate::todo::Todo;
use crate::fm::FileId;
use crate::purge::{Purge, Purges};

use std::hint;

#[derive(Eq, Copy, Clone, Debug, PartialEq)]
pub enum Mode {
    Purging,
    Listing,
    Reporting
}

impl Mode {
    #[must_use]
    #[inline(always)]
    pub const fn to_string_past(&self) -> &str {
        match self {
            Self::Purging   => "purged",
            Self::Reporting => "reported",
            Self::Listing   => "listed",
        }
    }

    #[must_use]
    #[inline(always)]
    pub const fn to_string_present(&self) -> &str {
        match self {
            Self::Purging   => "purge",
            Self::Reporting => "report",
            Self::Listing   => "list",
        }
    }

    #[must_use]
    #[inline(always)]
    pub const fn to_string_actioning(&self) -> &str {
        match self {
            Self::Purging   => "purging",
            Self::Reporting => "reporting",
            Self::Listing   => "listing",
        }
    }

    pub fn print_finish_msg(
        &self,
        found: usize,
        processed: usize
    ) {
        if found == 0 {
            println!("[no todoʼs to {}]", self.to_string_present());
        } else {
            println! {
                "[{processed}/{found}] todoʼs {what}",
                what = self.to_string_past()
            }
        }
    }
}

pub enum ModeValue {
    Reporting(Vec<Todo>),
    Purging(Purges),
    Listing(Vec<Todo>)
}

impl ModeValue {
    const RESERVE_CAP: usize = 4;

    #[inline(always)]
    #[must_use]
    pub fn new(mode: Mode, file_id: FileId) -> Self {
        match mode {
            Mode::Purging => Self::Purging(
                Purges::with_capacity(Self::RESERVE_CAP, file_id)
            ),

            Mode::Reporting => Self::Reporting(
                Vec::with_capacity(Self::RESERVE_CAP)
            ),

            Mode::Listing => Self::Listing(
                Vec::with_capacity(Self::RESERVE_CAP)
            ),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Purging(v)   => v.is_empty(),
            Self::Reporting(v) => v.is_empty(),
            Self::Listing(v) => v.is_empty(),
        }
    }

    #[track_caller]
    #[inline(always)]
    pub fn push_purge(&mut self, purge: Purge) {
        match self {
            Self::Purging(ps) => ps.push(purge),
            Self::Reporting(_) | Self::Listing(_) => unsafe { hint::unreachable_unchecked() }
        }
    }

    #[track_caller]
    #[inline(always)]
    pub fn push_todo(&mut self, todo: Todo) {
        match self {
            Self::Reporting(todos) | Self::Listing(todos) => todos.push(todo),
            Self::Purging(_) => unsafe { hint::unreachable_unchecked() }
        }
    }
}
