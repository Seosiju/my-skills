//! Machine-local writer lock. Readers use atomic state replacement instead.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use fs4::{FileExt, TryLockError};

use crate::commands::STATE_BUSY_PREFIX;
use crate::platform::paths::display_path;
use crate::{Error, Result};

pub(crate) struct StateLock(File);

impl StateLock {
    pub(crate) fn acquire(state: &Path) -> Result<Self> {
        let path = state.with_file_name("state.json.lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| Error::io(parent, err))?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|err| Error::io(&path, err))?;
        let max_wait_ms = 30_000;
        let timeout = Duration::from_millis(
            std::env::var("MY_SKILLS_STATE_LOCK_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(max_wait_ms)
                .min(max_wait_ms),
        );
        let start = Instant::now();
        let mut warned = false;
        loop {
            match FileExt::try_lock(&file) {
                Ok(()) => return Ok(Self(file)),
                Err(TryLockError::Error(err)) => return Err(Error::io(&path, err)),
                Err(TryLockError::WouldBlock) => {}
            }
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return Err(Error::State(format!(
                    "{STATE_BUSY_PREFIX}{}",
                    display_path(&path)
                )));
            }
            if elapsed >= Duration::from_secs(1) && !warned {
                eprintln!("Waiting for another my-skills command to finish...");
                warned = true;
            }
            thread::sleep(Duration::from_millis(50).min(timeout - elapsed));
        }
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        // Keep the file in place for other writers that already opened it.
        let _ = FileExt::unlock(&self.0);
    }
}
