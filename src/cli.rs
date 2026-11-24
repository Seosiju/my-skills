//! Command-line surface. Owned by the integrator.
//!
//! Commands and flags match v0.4.1 (spec §6.9), plus the v1 additions:
//! `sync --no-prune/--dry-run/--quiet`, `convert`, `hooks`, and
//! `doctor --json`. Each command's handler lives in `crate::commands`.

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::commands;
use crate::registry::manifest::InstallMode;

#[derive(Debug, Parser)]
#[command(
    name = "my-skills",
    version,
    about = "Personal skill drive: one canonical registry, linked into every agent host.",
    after_help = "\
Registry: $MY_SKILLS_ROOT, else ./my-skills.toml (or a parent), else the active
registry saved by `set-root`. Commands that write refuse to run from a registry
that is not the active one; run `set-root` there or set MY_SKILLS_ROOT.
Read-only commands and --dry-run never change the saved active registry.

Hosts: without --host, a command acts on every enabled [targets.*] host.

Exit codes: 0 ok; 1 blocked or failed check; 2 usage or configuration error.
--dry-run exits 1 only on validation failure; read `blocked` in --json output,
or use `sync --check`, which exits 1 when anything is out of date."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModeArg {
    Link,
    Copy,
}

impl From<ModeArg> for InstallMode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::Link => InstallMode::Link,
            ModeArg::Copy => InstallMode::Copy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Channel {
    Stable,
    Main,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate canonical skills against the Agent Skills standard
    Validate(ValidateArgs),
    /// Audit canonical skills for agent-skill security risks
    Audit(AuditArgs),
    /// Report environment, hosts, registry, hooks, and manifest health
    Doctor(DoctorArgs),
    /// Update the installed my-skills binary
    Update(UpdateArgs),
    /// Connect enabled skills for this machine
    Bootstrap(BootstrapArgs),
    /// Create a private canonical skill registry scaffold
    InitRegistry(InitRegistryArgs),
    /// Set the active registry root used when running outside a registry
    SetRoot(SetRootArgs),
    /// Connect skills into host directories (link by default)
    Install(InstallArgs),
    /// Plan sharing host-local skills into canonical
    Share(ShareArgs),
    /// Enable a registered skill in the manifest
    Enable(SkillNameArgs),
    /// Disable a registered skill in the manifest
    Disable(SkillNameArgs),
    /// Show connection status per skill and host
    Status(StatusArgs),
    /// List registered canonical skills
    Skills(SkillsArgs),
    /// Reconcile host directories with the manifest (connect and prune)
    Sync(SyncArgs),
    /// Remove managed connections (never the canonical source)
    Uninstall(UninstallArgs),
    /// Import an external skill directory into canonical skills/
    Import(ImportArgs),
    /// Print a skill's shared machine-local data directory
    DataPath(DataPathArgs),
    /// Switch existing connections between link and copy
    Convert(ConvertArgs),
    /// Install, remove, or inspect the registry's git hooks
    Hooks(HooksArgs),
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Skill to validate (default: every skill directory under skills/)
    pub skill: Option<String>,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// Skill to audit (default: every enabled skill)
    pub skill: Option<String>,
    /// Include disabled skills
    #[arg(long)]
    pub all: bool,
    /// Machine-readable findings
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Skip the network check for a newer my-skills release
    #[arg(long)]
    pub no_update_check: bool,
    /// Machine-readable output (v1)
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Release channel to update from
    #[arg(long, value_enum, default_value = "stable")]
    pub channel: Channel,
    /// Only report whether a newer version exists
    #[arg(long)]
    pub check: bool,
    /// Show what would be downloaded and replaced; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct BootstrapArgs {
    /// Show the plan; write nothing
    #[arg(long)]
    pub dry_run: bool,
    /// Accepted for v0.4.1 compatibility; v1 has no tool install step
    #[arg(long)]
    pub skip_tool_install: bool,
    /// Skip connecting skills
    #[arg(long)]
    pub skip_skill_install: bool,
    /// Connect only to this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Also connect disabled skills
    #[arg(long)]
    pub all: bool,
    /// Override the install mode for new connections
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
}

#[derive(Debug, Args)]
pub struct InitRegistryArgs {
    /// Where to create the registry (default: ~/my-agent-skills; asked in a terminal)
    pub path: Option<String>,
    /// Include the seed skills (the default)
    #[arg(long, conflicts_with = "no_defaults")]
    pub with_defaults: bool,
    /// Create an empty registry without seed skills
    #[arg(long)]
    pub no_defaults: bool,
    /// Skip `git init` and the initial commit
    #[arg(long)]
    pub no_git: bool,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SetRootArgs {
    /// Registry to make active (default: the current directory's registry)
    pub path: Option<String>,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Skill to connect (default: every enabled skill)
    pub skill: Option<String>,
    /// Connect only to this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Also connect disabled skills
    #[arg(long)]
    pub all: bool,
    /// Override the install mode for new connections (link or copy)
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
    /// Show the plan, validation, and audit result; write nothing.
    /// Exits 1 only on validation failure
    #[arg(long)]
    pub dry_run: bool,
    /// Machine-readable plan with `blocked` and `applicable`; requires --dry-run
    #[arg(long)]
    pub json: bool,
    /// Connect even if the security audit would block
    #[arg(long)]
    pub skip_audit: bool,
    /// Confirm a write to more than one host
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ShareArgs {
    /// Host-local skill to copy into the registry (required unless --plan)
    pub skill: Option<String>,
    /// Host whose local skills to read
    #[arg(long = "from")]
    pub from_host: String,
    /// List which host-local skills could be shared; write nothing
    #[arg(long)]
    pub plan: bool,
    /// Machine-readable plan output
    #[arg(long)]
    pub json: bool,
    /// Register the shared skill as enabled
    #[arg(long, conflicts_with = "disable")]
    pub enable: bool,
    /// Register the shared skill as disabled
    #[arg(long)]
    pub disable: bool,
    /// Overwrite a different canonical skill with the same name
    #[arg(long)]
    pub force: bool,
    /// Share even if the security audit would block (prints a warning)
    #[arg(long)]
    pub skip_audit: bool,
    /// Same as --plan: show what would be shared; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SkillNameArgs {
    /// Registered skill name
    pub skill: String,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Machine-readable output (v1)
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SkillsArgs {
    /// Machine-readable output (per-host connection status is always included)
    #[arg(long)]
    pub json: bool,
    /// Only skills that target this host, with its status column
    #[arg(long)]
    pub host: Option<String>,
    /// Only enabled skills
    #[arg(long, conflicts_with = "disabled")]
    pub enabled: bool,
    /// Only disabled skills
    #[arg(long)]
    pub disabled: bool,
    /// Add governance details: audit result and provenance (source, revision)
    #[arg(long)]
    pub with_status: bool,
}

#[derive(Debug, Default, Args)]
pub struct SyncArgs {
    /// Sync only this skill (default: every registered skill; prunes orphans)
    pub skill: Option<String>,
    /// Sync only this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Also connect disabled skills
    #[arg(long)]
    pub all: bool,
    /// Report drift only; change nothing. Exits 1 when anything is out of date
    #[arg(long)]
    pub check: bool,
    /// Confirm a write to more than one host
    #[arg(long)]
    pub yes: bool,
    /// Connect even if the security audit would block
    #[arg(long)]
    pub skip_audit: bool,
    /// Keep connections the manifest no longer wants (sync prunes them by default)
    #[arg(long)]
    pub no_prune: bool,
    /// Show the plan, validation, and audit result; write nothing.
    /// Exits 1 only on validation failure
    #[arg(long)]
    pub dry_run: bool,
    /// Machine-readable plan; requires --dry-run
    #[arg(long)]
    pub json: bool,
    /// Print only changes and problems; for git hooks (v1)
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Skill whose connections to remove (required); the registry copy stays
    pub skill: Option<String>,
    /// Remove only from this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Confirm a write to more than one host
    #[arg(long)]
    pub yes: bool,
    /// Show what would be removed; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    /// Skill directory (containing SKILL.md) to copy into skills/
    pub source: String,
    /// Overwrite a different canonical skill with the same name
    #[arg(long)]
    pub force: bool,
    /// Register the imported skill as enabled
    #[arg(long)]
    pub enable: bool,
    /// Import even if the security audit would block
    #[arg(long)]
    pub skip_audit: bool,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct DataPathArgs {
    /// Skill whose machine-local data directory to print
    pub skill: String,
    /// Create the directory if it does not exist
    #[arg(long)]
    pub create: bool,
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Skill to convert (default: every registered skill's connections)
    pub skill: Option<String>,
    /// Target mode
    #[arg(long, value_enum)]
    pub to: ModeArg,
    /// Convert only on this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Accepted for compatibility; convert already covers every registered skill
    #[arg(long)]
    pub all: bool,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm a write to more than one host
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct HooksArgs {
    #[command(subcommand)]
    pub action: HooksAction,
}

#[derive(Debug, Subcommand)]
pub enum HooksAction {
    /// Write the managed hooks into the active registry
    Install(HooksInstallArgs),
    /// Remove the managed hooks (hooks without the marker are left alone)
    Uninstall(HooksUninstallArgs),
    /// Show which hooks are installed and whether my-skills is reachable
    Status,
    /// Hook entry points; called by the installed hook scripts
    #[command(hide = true)]
    Run(HooksRunArgs),
}

#[derive(Debug, Args)]
pub struct HooksInstallArgs {
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct HooksUninstallArgs {
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct HooksRunArgs {
    /// pre-commit | post-merge | post-rewrite | post-checkout
    pub hook: String,
    /// Arguments git passed to the hook
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub git_args: Vec<String>,
}

pub fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Command::Validate(args) => commands::inspect::validate(&args),
        Command::Audit(args) => commands::inspect::audit(&args),
        Command::Doctor(args) => commands::doctor::run(&args),
        Command::Update(args) => commands::update::run(&args),
        Command::Bootstrap(args) => commands::connect::bootstrap(&args),
        Command::InitRegistry(args) => commands::registry::init_registry(&args),
        Command::SetRoot(args) => commands::inspect::set_root(&args),
        Command::Install(args) => commands::connect::install(&args),
        Command::Share(args) => commands::registry::share(&args),
        Command::Enable(args) => commands::registry::enable(&args),
        Command::Disable(args) => commands::registry::disable(&args),
        Command::Status(args) => commands::inspect::status(&args),
        Command::Skills(args) => commands::registry::skills(&args),
        Command::Sync(args) => commands::connect::sync(&args),
        Command::Uninstall(args) => commands::connect::uninstall(&args),
        Command::Import(args) => commands::registry::import(&args),
        Command::DataPath(args) => commands::inspect::data_path(&args),
        Command::Convert(args) => commands::connect::convert(&args),
        Command::Hooks(args) => commands::hooks::run(&args),
    }
}
