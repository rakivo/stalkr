use crate::config::Config;
use crate::fm::{FileId, FileManager};

use std::ops::Range;
use std::fs::OpenOptions;

use serde_json::Value;

pub type Purges = Box<[Purge]>;

pub struct Purge {
    pub issue_number: u64,
    pub range: Range<usize>
}

fn issue_closed(config: &Config, issue_number: u64) -> anyhow::Result<bool> {
    let url = config.get_issue_api_url(issue_number);

    let client = reqwest::blocking::Client::builder()
        .user_agent("stalkr-todo-bot")
        .build()?;

    let issue_state = client
        .get(&url)
        .bearer_auth(&config.gh_token)
        .header("Accept", "application/vnd.github.v3+json")
        .send()?
        .error_for_status()?
        .json::<Value>()?;

    let state = issue_state.get("state").and_then(|s| s.as_str()).ok_or_else(|| {
        anyhow::anyhow!("could not parse issue state")
    })?;

    Ok(state == "closed")
}

pub fn purge(file_id: FileId, purges: Purges, config: &Config, fm: &FileManager) -> anyhow::Result<()> {
    if purges.is_empty() {
        return Ok(())
    }

    let mut purges = purges.into_vec();

    purges.sort_by_key(|p| p.range.start);

    let mut new_len = fm.get_file_unchecked(file_id).meta.len() as usize;

    let file_path = fm.get_file_path_unchecked(file_id).to_owned();
    let mut mmap = fm.get_mmap_or_remmap_file_mut(file_id, new_len)?;

    for Purge { range, issue_number } in purges.into_iter().rev() {
        if !issue_closed(config, issue_number)? {
            continue
        }

        let start = range.start;
        let end   = range.end;

        debug_assert!{
            end <= new_len,
            "purge range {range:?} past end {new_len}"
        };

        let len = end - start;

        // how many bytes follow this hole right now?
        let tail_len = new_len - end;

        // slide the tail block down on top of the hole
        mmap.copy_within(end..end + tail_len, start);

        // reduce the effective length
        new_len -= len;
    }

    // flush and shrink
    mmap.flush()?;
    drop(mmap);

    OpenOptions::new()
        .write(true)
        .open(&file_path)?
        .set_len(new_len as _)?;

    Ok(())
}

