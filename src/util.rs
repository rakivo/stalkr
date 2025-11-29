use std::path::{Path, PathBuf};
use std::borrow::Cow;
use std::{fs, mem, slice, str};
use std::io::{self, Write};

#[inline]
pub fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
    _ = io::stdout().flush();
}

#[inline]
#[must_use]
pub fn ask_input(prompt: &str) -> String {
    print!("{prompt} ");
    _ = io::stdout().flush();

    let mut buf = String::new();
    io::stdin().read_line(&mut buf).expect("[couldn't ask input]");
    buf
}

#[inline]
#[must_use]
pub fn trim_comment_start(s: &str) -> &str {
    s
        .trim_start()
        .trim_start_matches("--")
        .trim_start_matches("//")
        .trim_start_matches('#')
        .trim_start_matches("/*")
        .trim_start()
}

#[inline]
#[must_use]
pub fn is_line_a_comment(h_: &str) -> Option<usize> {
    let h = h_.trim_start();

    let first_byte = h.as_bytes().first()?;
    let second_byte = || h.as_bytes().get(1);

    let comment_offset = match first_byte {
        b'#' => 1,
        b'-' if matches!(second_byte()?, b'-') => 2,
        b'/' if matches!(second_byte()?, b'/' | b'*') => 2,
        _ => return None
    };

    Some(h_.len() - h.len() + comment_offset)
}

#[inline]
#[allow(unused)]
#[must_use]
pub fn extract_text_from_a_comment(h: &str) -> Option<&str> {
    let comment_end = is_line_a_comment(h)?;
    Some(h[comment_end..].trim())
}

// NOTE: drops any excess Vec capacity without freeing it.
//       This "leaks" a little memory, proportional to (capacity - len).
#[inline]
#[must_use]
pub fn vec_into_boxed_slice_norealloc<T>(mut v: Vec<T>) -> Box<[T]> {
    let len = v.len();
    let ptr = v.as_mut_ptr();

    mem::forget(v);

    unsafe {
        Box::from_raw(slice::from_raw_parts_mut(ptr, len))
    }
}

// NOTE: drops any excess Vec capacity without freeing it.
//       This "leaks" a little memory, proportional to (capacity - len).
#[inline]
#[must_use]
pub fn string_into_boxed_str_norealloc(s: String) -> Box<str> {
    let s = s.into_bytes();
    let s = vec_into_boxed_slice_norealloc(s);

    let len = s.len();
    let ptr = Box::into_raw(s).cast();

    unsafe {
        let slice = slice::from_raw_parts_mut(ptr, len);

        // SAFETY: String `s` constains valid UTF-8 bytes
        let str = str::from_utf8_unchecked_mut(slice);

        Box::from_raw(str)
    }
}

#[must_use]
pub fn balance_concurrency(cpu_count: usize) -> (usize, usize) {
    let cpu_count = cpu_count.max(2);

    // reserve some threads for async work
    let reserved_for_async = if cpu_count >= 8 { 2 } else { 1 };

    // also leave one thread for `spawn_blocking` work (mmap, etc)
    let reserved_for_blocking = 1;

    // total non rayon threads
    let reserved_total = reserved_for_async + reserved_for_blocking;

    // threads available for Rayon parallel processing
    let rayon_threads = cpu_count.saturating_sub(reserved_total).max(1);

    // async max concurrency for HTTP issuing depends on how "fast" the network is
    // Usually <= # of async threads is fine, but a little overbooking is ok
    let max_concurrency = reserved_for_async * 8;

    (rayon_threads, max_concurrency)
}

#[must_use]
pub fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    // find the "github.com/" or "github.com:" pivot
    let pivot = url.find("github.com/").or_else(|| url.find("github.com:"))?;
    // slice just after the slash/colon
    let rest = &url[pivot + "github.com/".len()..];
    // split into owner and repo.git?
    let mut parts = rest.splitn(2, '/');

    let owner = parts.next()?.to_owned();
    let mut repo = parts.next()?.to_owned();

    // strip optional ".git" suffix
    if Path::new(&repo)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("git"))
    {
        repo.truncate(repo.len() - 4);
    }

    Some((owner, repo))
}

#[must_use]
pub fn get_git_origin_url(mut dir: PathBuf, remote: &str) -> Option<String> {
    loop {
        let config = dir.join(".git/config");
        if config.exists() {
            let contents = fs::read_to_string(&config).ok()?;

            // First, try to find the requested remote
            if let Some(url) = find_remote_url(&contents, remote) {
                return Some(url);
            }

            // Fallback 1: Try pushDefault
            if let Some(push_default) = find_push_default(&contents) {
                eprintln!("[falling back to pushDefault]: {push_default}");
                if let Some(url) = find_remote_url(&contents, &push_default) {
                    return Some(url);
                }
            }

            // Fallback 2: Try current branch's remote
            if let Some(branch_remote) = find_current_branch_remote(&dir, &contents) {
                println!("[falling back to branch remote]: {branch_remote}");
                if let Some(url) = find_remote_url(&contents, &branch_remote) {
                    return Some(url);
                }
            }

            // Fallback 3: Use any available remote
            if let Some(url) = find_any_remote_url(&contents) {
                println!("[falling back to first available remote]");
                return Some(url);
            }

            break;
        }

        // go up
        if !dir.pop() { break }
    }
    None
}

#[must_use]
fn find_remote_url(contents: &str, remote: &str) -> Option<String> {
    let mut in_target_remote = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("[remote \"") {
            in_target_remote = line.contains(&format!("\"{remote}\""));
        } else if in_target_remote && line.starts_with("url") {
            return line.split('=')
                .nth(1)
                .map(|s| s.trim().to_owned());
        }
    }
    None
}

#[must_use]
fn find_push_default(contents: &str) -> Option<String> {
    let mut in_remote_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("[remote]") {
            in_remote_section = true;
        } else if line.starts_with('[') {
            in_remote_section = false;
        } else if in_remote_section && line.starts_with("pushDefault") {
            return line.split('=')
                .nth(1)
                .map(|s| s.trim().to_owned());
        }
    }
    None
}

#[must_use]
fn find_current_branch_remote(dir: &Path, contents: &str) -> Option<String> {
    // Read current branch from HEAD
    let head = dir.join(".git/HEAD");
    let head_contents = fs::read_to_string(head).ok()?;
    let branch_name = head_contents
        .strip_prefix("ref: refs/heads/")?
        .trim();

    // Find remote for this branch in config
    let mut in_branch_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with(&format!("[branch \"{branch_name}\"]")) {
            in_branch_section = true;
        } else if line.starts_with('[') {
            in_branch_section = false;
        } else if in_branch_section && line.starts_with("remote") {
            return line.split('=')
                .nth(1)
                .map(|s| s.trim().to_owned());
        }
    }
    None
}

#[must_use]
fn find_any_remote_url(contents: &str) -> Option<String> {
    let mut in_remote = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("[remote \"") {
            in_remote = true;
        } else if in_remote && line.starts_with("url") {
            return line.split('=')
                .nth(1)
                .map(|s| s.trim().to_owned());
        }
    }
    None
}

#[must_use]
pub fn truncate_path(path: &str, line_number: u32, max_len: usize) -> Cow<'_, str> {
    let line_number_len = line_number.to_string().len() + 1; // ':'

    let available_for_path = max_len.saturating_sub(line_number_len);

    if path.len() <= available_for_path {
        return path.into()
    }

    // try to keep the filename and some parent directories
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() <= 1 {
        return format!(
            "...{p}",
            p = &path[path.len().saturating_sub(available_for_path - 3)..]
        ).into();
    }

    let Some(filename) = parts.last() else {
        return path.into();
    };

    let mut remaining_len = available_for_path - 3; // account for "..."

    // always include the filename
    if filename.len() > remaining_len {
        return format!(
            "...{p}",
            p = &filename[filename.len().saturating_sub(remaining_len)..]
        ).into();
    }

    let mut ret = String::from("...");
    ret.reserve(filename.len() + 1);
    ret.push('/');
    ret.push_str(filename);
    remaining_len -= filename.len() + 1;

    // add parent directories from right to left if they fit
    for parent in parts.iter().rev().skip(1) {
        if parent.len() + 1 > remaining_len {
            break;
        }
        ret.insert_str(4, &format!("{parent}/")); // insert after ".../"
        remaining_len -= parent.len() + 1;
    }

    ret.into()
}

macro_rules! make_spawn {
    (
        $rx_inner_ty: ty,
        $(#[$meta:meta]) *
        $vis:vis fn new(
            $($arg_name:ident : $arg_ty:ty), * $(,)?
        ) -> Self
        $body: block
    ) => {
        $(#[$meta]) *
        $vis fn new(
            $($arg_name : $arg_ty), *
        ) -> Self
        $body

        /// Spawn the issuing loop and return its `JoinHandle`.
        ///
        /// Takes *all* of `new`'s parameters, plus the `issue_rx` at the end.
        #[allow(unused)]
        $vis fn spawn(
            $($arg_name : $arg_ty, ) *
            rx: tokio::sync::mpsc::UnboundedReceiver<$rx_inner_ty>
        ) -> tokio::task::JoinHandle<()> {
            let mut me = Self::new($($arg_name), *);
            tokio::spawn(async move { me.run(rx).await; })
        }
    };
}
