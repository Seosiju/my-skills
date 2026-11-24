//! Machine-level locations shared by every module (spec §7.1).
//!
//! Environment overrides are honored first so tests can run in a temporary
//! HOME without touching the real host directories.

use std::path::{Path, PathBuf};

/// The user's home directory: `HOME` (all platforms, so tests can override),
/// then `USERPROFILE` on Windows, then the OS answer.
pub fn home_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home);
    }
    #[cfg(windows)]
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
        return PathBuf::from(profile);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `$XDG_STATE_HOME` or `~/.local/state`. v0.4.1 uses this on Windows too;
/// spec Q3 decides whether v1 moves Windows elsewhere.
pub fn state_home() -> PathBuf {
    env_dir("XDG_STATE_HOME").unwrap_or_else(|| home_dir().join(".local").join("state"))
}

/// `$XDG_CONFIG_HOME` or `~/.config` (active-root cache, as in v0.4.1).
pub fn config_home() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home_dir().join(".config"))
}

/// Base of per-skill data (spec §8.4). POSIX: `$XDG_DATA_HOME/my-skills` or
/// `~/.local/share/my-skills`. Windows: `%LOCALAPPDATA%\my-skills\data`.
#[cfg(windows)]
pub fn data_root() -> PathBuf {
    env_dir("LOCALAPPDATA")
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
        .join("my-skills")
        .join("data")
}

/// See the Windows variant above for the layout.
#[cfg(not(windows))]
pub fn data_root() -> PathBuf {
    env_dir("XDG_DATA_HOME")
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
        .join("my-skills")
}

/// Strip Windows verbatim prefixes (`\\?\` and `\\?\UNC\`) for user-facing display.
pub fn strip_verbatim_prefix(text: &str) -> String {
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }
    if let Some(disk) = text.strip_prefix(r"\\?\") {
        return disk.to_owned();
    }
    text.to_owned()
}

/// Show Windows paths with native separators and without verbatim prefixes.
/// Filesystem calls still use the original paths.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        strip_verbatim_prefix(&text.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_verbatim_prefix_handles_windows_and_posix_paths() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Users\example\my-agent-skills"),
            r"C:\Users\example\my-agent-skills"
        );
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\my-agent-skills"),
            r"\\server\share\my-agent-skills"
        );
        assert_eq!(
            strip_verbatim_prefix(r"C:\Users\example\my-agent-skills"),
            r"C:\Users\example\my-agent-skills"
        );
        assert_eq!(
            strip_verbatim_prefix("/Users/example/my-agent-skills"),
            "/Users/example/my-agent-skills"
        );
    }
}
