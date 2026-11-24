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
    about = "Personal skill drive: one canonical registry, linked into every agent host."
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
    pub skill: Option<String>,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    pub skill: Option<String>,
    #[arg(long)]
    pub all: bool,
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
    #[arg(long, value_enum, default_value = "stable")]
    pub channel: Channel,
    #[arg(long)]
    pub check: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct BootstrapArgs {
    #[arg(long)]
    pub dry_run: bool,
    /// Accepted for v0.4.1 compatibility; v1 has no tool install step
    #[arg(long)]
    pub skip_tool_install: bool,
    #[arg(long)]
    pub skip_skill_install: bool,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
}

#[derive(Debug, Args)]
pub struct InitRegistryArgs {
    pub path: Option<String>,
    #[arg(long, conflicts_with = "no_defaults")]
    pub with_defaults: bool,
    #[arg(long)]
    pub no_defaults: bool,
    #[arg(long)]
    pub no_git: bool,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SetRootArgs {
    pub path: Option<String>,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    pub skill: Option<String>,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub skip_audit: bool,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ShareArgs {
    pub skill: Option<String>,
    #[arg(long = "from")]
    pub from_host: String,
    #[arg(long)]
    pub plan: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long, conflicts_with = "disable")]
    pub enable: bool,
    #[arg(long)]
    pub disable: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub skip_audit: bool,
    /// Same as --plan: show what would be shared; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SkillNameArgs {
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
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long, conflicts_with = "disabled")]
    pub enabled: bool,
    #[arg(long)]
    pub disabled: bool,
    #[arg(long)]
    pub with_status: bool,
}

#[derive(Debug, Default, Args)]
pub struct SyncArgs {
    pub skill: Option<String>,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub all: bool,
    /// Report drift only; change nothing
    #[arg(long)]
    pub check: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub skip_audit: bool,
    /// Keep connections the manifest no longer wants (v1)
    #[arg(long)]
    pub no_prune: bool,
    /// Print the plan without writing (v1)
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
    pub skill: Option<String>,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    pub source: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub enable: bool,
    #[arg(long)]
    pub skip_audit: bool,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct DataPathArgs {
    pub skill: String,
    #[arg(long)]
    pub create: bool,
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    pub skill: Option<String>,
    #[arg(long, value_enum)]
    pub to: ModeArg,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub dry_run: bool,
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
