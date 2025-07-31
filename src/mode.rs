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

pub enum ModeValue {
    Reporting(Vec<Todo>),
    Purging(Purges),
}

impl ModeValue {
    const RESERVE_CAP: usize = 4;

    #[inline(always)]
    pub fn new(mode: Mode, file_id: FileId) -> Self {
        match mode {
            Mode::Purging => Self::Purging(
                Purges::with_capacity(Self::RESERVE_CAP, file_id)
            ),

            Mode::Reporting => Self::Reporting(
                Vec::with_capacity(Self::RESERVE_CAP)
            ),

            _ => todo!()
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Purging(v)   => v.is_empty(),
            Self::Reporting(v) => v.is_empty(),
        }
    }

    #[track_caller]
    #[inline(always)]
    pub fn push_purge(&mut self, purge: Purge) {
        match self {
            Self::Purging(ps) => ps.push(purge),
            _ => unsafe { hint::unreachable_unchecked() }
        }
    }

    #[track_caller]
    #[inline(always)]
    pub fn push_todo(&mut self, todo: Todo) {
        match self {
            Self::Reporting(todos) => todos.push(todo),
            _ => unsafe { hint::unreachable_unchecked() }
        }
    }
}

