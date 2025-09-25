use std::sync::Mutex;
use std::process::Command;

use anyhow::bail;

pub struct GitLocker {
    mutex: Mutex<()>,
}

impl GitLocker {
    #[inline(always)]
    pub const fn new() -> Self {
        Self { mutex: Mutex::new(()) }
    }

    pub fn commit_changes(&self, path: &str, msg: &str) -> anyhow::Result<()> {
        let _g = self.mutex.lock().unwrap();
        let status = Command::new("git").arg("add").arg(path).status()?;

        if !status.success() {
            bail!("git add failed")
        }

        let status = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(msg)
            .status()?;

        if !status.success() {
            bail!("git commit failed")
        }

        Ok(())
    }
}
