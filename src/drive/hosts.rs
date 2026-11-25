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
    pub skill_display: SkillDisplay,
    pub default_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDisplay {
    pub version: &'static str,
    pub measured_on: &'static str,
    pub uses_folder_name: bool,
    pub name_limit: Option<usize>,
    pub description_limit: Option<DescriptionLimit>,
    pub missing_description: MissingDescription,
    pub requires_frontmatter: bool,
    pub accepts_bom: bool,
    pub frontmatter_keep_fields: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptionLimit {
    pub max_chars: usize,
    pub suffix: &'static str,
    pub extra_field: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingDescription {
    FirstBodyLine,
    Hidden,
    Blank,
}

const MEASURED_ON: &str = "2026-11-25";

pub const CLAUDE: HostConfig = HostConfig {
    name: "claude",
    display_name: "Claude Code",
    detect_commands: &["claude"],
    default_user_path: "~/.claude/skills",
    default_project_path: ".claude/skills",
    reload_hint: "Claude Code picks up skills on restart; relaunch to see new skills.",
    skill_display: SkillDisplay {
        version: "2.1.318",
        measured_on: MEASURED_ON,
        uses_folder_name: true,
        name_limit: None,
        description_limit: Some(DescriptionLimit {
            max_chars: 1536,
            suffix: "…",
            extra_field: Some("when_to_use"),
        }),
        missing_description: MissingDescription::FirstBodyLine,
        requires_frontmatter: false,
        accepts_bom: true,
        frontmatter_keep_fields: &["name", "description", "when_to_use"],
    },
    default_enabled: true,
};

pub const CODEX: HostConfig = HostConfig {
    name: "codex",
    display_name: "Codex",
    detect_commands: &["codex"],
    default_user_path: "~/.agents/skills",
    default_project_path: ".agents/skills",
    reload_hint: "Codex detects skill changes automatically; restart if needed.",
    skill_display: SkillDisplay {
        version: "0.172.0",
        measured_on: MEASURED_ON,
        uses_folder_name: false,
        name_limit: Some(64),
        description_limit: Some(DescriptionLimit {
            max_chars: 1024,
            suffix: "...",
            extra_field: None,
        }),
        missing_description: MissingDescription::Hidden,
        requires_frontmatter: true,
        accepts_bom: false,
        frontmatter_keep_fields: &["name", "description"],
    },
    default_enabled: true,
};

pub const AGY: HostConfig = HostConfig {
    name: "agy",
    display_name: "Antigravity CLI",
    detect_commands: &["agy"],
    default_user_path: "~/.gemini/config/skills",
    default_project_path: ".agents/skills",
    reload_hint: "Start a new Antigravity CLI session and use /skills to verify discovery.",
    skill_display: SkillDisplay {
        version: "1.5.16",
        measured_on: MEASURED_ON,
        uses_folder_name: false,
        name_limit: None,
        description_limit: None,
        missing_description: MissingDescription::Blank,
        requires_frontmatter: true,
        accepts_bom: true,
        frontmatter_keep_fields: &["name", "description"],
    },
    default_enabled: false,
};

pub const ALL: [&HostConfig; 3] = [&CLAUDE, &CODEX, &AGY];

pub fn get_host(name: &str) -> Option<&'static HostConfig> {
    ALL.iter().copied().find(|h| h.name == name)
}
