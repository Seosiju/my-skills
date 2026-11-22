//! Built-in host registry. Mirrors v0.4.1 `hosts/*.py`.
//!
//! A manifest may declare any other target with an explicit `path`
//! (spec §3.4). Such targets have no `HostConfig`; host-specific validation
//! is skipped for them, as in v0.4.1.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub name: &'static str,
    pub display_name: &'static str,
    pub detect_commands: &'static [&'static str],
    /// `~`-relative user scope path. Windows paths: spec §7.1, Q1.
    pub default_user_path: &'static str,
    pub default_project_path: &'static str,
    pub reload_hint: &'static str,
    pub description_max_chars: usize,
    pub frontmatter_keep_fields: &'static [&'static str],
    pub default_enabled: bool,
}

const KEEP_FIELDS: &[&str] = &["name", "description"];

pub const CLAUDE: HostConfig = HostConfig {
    name: "claude",
    display_name: "Claude Code",
    detect_commands: &["claude"],
    default_user_path: "~/.claude/skills",
    default_project_path: ".claude/skills",
    reload_hint: "Claude Code picks up skills on restart; relaunch to see new skills.",
    description_max_chars: 768,
    frontmatter_keep_fields: KEEP_FIELDS,
    default_enabled: true,
};

pub const CODEX: HostConfig = HostConfig {
    name: "codex",
    display_name: "Codex",
    detect_commands: &["codex"],
    default_user_path: "~/.agents/skills",
    default_project_path: ".agents/skills",
    reload_hint: "Codex detects skill changes automatically; restart if needed.",
    description_max_chars: 768,
    frontmatter_keep_fields: KEEP_FIELDS,
    default_enabled: true,
};

pub const HERMES: HostConfig = HostConfig {
    name: "hermes",
    display_name: "Hermes",
    detect_commands: &["hermes"],
    default_user_path: "~/.hermes/skills",
    default_project_path: ".hermes/skills",
    reload_hint: "Hermes reloads skills on restart; the agent may also edit them.",
    description_max_chars: 768,
    frontmatter_keep_fields: KEEP_FIELDS,
    default_enabled: true,
};

pub const AGY: HostConfig = HostConfig {
    name: "agy",
    display_name: "Antigravity CLI",
    detect_commands: &["agy"],
    default_user_path: "~/.gemini/config/skills",
    default_project_path: ".agents/skills",
    reload_hint: "Start a new Antigravity CLI session and use /skills to verify discovery.",
    description_max_chars: 768,
    frontmatter_keep_fields: KEEP_FIELDS,
    default_enabled: false,
};

/// Registration order matches v0.4.1.
pub const ALL: [&HostConfig; 4] = [&CLAUDE, &CODEX, &HERMES, &AGY];

pub fn get_host(name: &str) -> Option<&'static HostConfig> {
    ALL.iter().copied().find(|h| h.name == name)
}
