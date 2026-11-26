//! Command-line surface. Owned by the integrator.
//!
//! Commands and flags match v0.4.1 (spec §6.9), plus the v1 additions:
//! `sync --no-prune/--dry-run/--quiet`, `convert`, `hooks`, and
//! `doctor --json`. Each command's handler lives in `crate::commands`.

use std::path::PathBuf;
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
Registry: --root, else $MY_SKILLS_ROOT, else ./my-skills.toml (or a parent),
else the active registry saved by `set-root`. Commands that write refuse to run
from a registry that is not the active one; pass --root, set MY_SKILLS_ROOT, or
run `set-root` there. Read-only commands, --dry-run, and --root never change the
saved active registry.

Skills: commands that take SKILL... also accept --group <GROUP> (repeatable),
which adds every skill whose manifest `group` matches.

Hosts: without --host, a command acts on every enabled [targets.*] host.

Exit codes: 0 ok; 1 blocked or failed check; 2 usage or configuration error.
`install --dry-run` and `sync --dry-run` exit 1 only on validation failure; read
`blocked` and `applicable` in their --json output, or use `sync --check`, which
exits 1 when anything is out of date."
)]
pub struct Cli {
    /// Use this registry for this command only (overrides MY_SKILLS_ROOT, the
    /// current directory, and the saved active registry; never saved)
    #[arg(long, global = true, value_name = "PATH")]
    pub root: Option<PathBuf>,
    /// Without a command, print help and, when no registry is set up yet,
    /// the first-run steps.
    #[command(subcommand)]
    pub command: Option<Command>,
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
    /// Enable registered skills in the manifest
    Enable(SkillNameArgs),
    /// Disable registered skills in the manifest
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
    /// Show how a host connection differs from its canonical skill (read-only)
    Diff(DiffArgs),
    /// Find the canonical source behind a skill name or a skill path (read-only)
    Resolve(ResolveArgs),
    /// Create a skill draft in skills/<name>/ and register it, disabled
    New(NewArgs),
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Skills to validate (default: every skill directory under skills/)
    pub skills: Vec<String>,
    /// Also validate every skill in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// Check host limits only for this host (default: every enabled host the
    /// skill connects to, as the pre-commit hook does)
    #[arg(long)]
    pub host: Option<String>,
    /// Machine-readable results
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Skip the network check for a newer my-skills release
    #[arg(long)]
    pub no_update_check: bool,
    /// Fast local check: skip the update check, registry git and hook status,
    /// full validation, and the link probe
    #[arg(long)]
    pub quick: bool,
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
    /// Do not install the my-skills git hooks
    #[arg(long)]
    pub no_hooks: bool,
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
    /// Do not install the my-skills git hooks
    #[arg(long)]
    pub no_hooks: bool,
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
    /// Skills to connect (default: every enabled skill)
    pub skills: Vec<String>,
    /// Also connect every skill in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// Connect only to this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Also connect disabled skills
    #[arg(long)]
    pub all: bool,
    /// Override the install mode for new connections (link or copy)
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
    /// Show the plan and validation; write nothing.
    /// Exits 1 only on validation failure
    #[arg(long)]
    pub dry_run: bool,
    /// Machine-readable output. With --dry-run: the plan, with `blocked` and
    /// `applicable`. Without it: the plan is applied and the result is
    /// reported per action (`result`), with `applied` and `summary`
    #[arg(long)]
    pub json: bool,
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
    /// Same as --plan: show what would be shared; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SkillNameArgs {
    /// Registered skill names (required unless --group is given)
    #[arg(required_unless_present = "groups")]
    pub skills: Vec<String>,
    /// Also select every skill in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Only these skills (default: every registered skill)
    pub skills: Vec<String>,
    /// Also show every skill in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// Machine-readable output (v1)
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SkillsArgs {
    /// Only these skills (default: every registered skill); combines with
    /// the filters below as AND
    pub skills: Vec<String>,
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
    /// Only skills in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// With --json: compact rows (name, enabled, hosts, group, status, plus
    /// error and governance when present); the table is unchanged
    #[arg(long)]
    pub brief: bool,
    /// Add provenance details (source type and trust tier)
    #[arg(long = "with-governance", alias = "with-status")]
    pub with_governance: bool,
}

#[derive(Debug, Default, Args)]
pub struct SyncArgs {
    /// Sync only these skills (default: every registered skill, which also
    /// prunes connections of skills no longer registered)
    pub skills: Vec<String>,
    /// Also sync every skill in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
    /// Sync only this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Also connect disabled skills
    #[arg(long)]
    pub all: bool,
    /// Report drift only; change nothing. Exits 1 when anything is out of date.
    /// Does not validate (use --dry-run --json for that). Takes
    /// precedence over --dry-run; not combinable with --json
    #[arg(long)]
    pub check: bool,
    /// Confirm a write to more than one host
    #[arg(long)]
    pub yes: bool,
    /// Keep connections the manifest no longer wants (sync prunes them by default)
    #[arg(long)]
    pub no_prune: bool,
    /// Show the plan and validation; write nothing.
    /// Exits 1 only on validation failure
    #[arg(long)]
    pub dry_run: bool,
    /// Machine-readable output. With --dry-run: the plan, with `blocked` and
    /// `applicable`. Without it: the plan is applied and the result is
    /// reported per action (`result`), with `applied` and `summary`
    #[arg(long)]
    pub json: bool,
    /// Print only changes and problems; for git hooks (v1)
    #[arg(long)]
    pub quiet: bool,
    /// Internal (not a flag): skills a git hook leaves untouched, neither
    /// connected nor pruned, because their files have uncommitted changes
    #[arg(skip)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Skills whose connections to remove (required unless --group); the
    /// registry copies stay
    pub skills: Vec<String>,
    /// Also select every skill in this manifest group (repeatable)
    #[arg(long = "group", value_name = "GROUP")]
    pub groups: Vec<String>,
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
pub struct DiffArgs {
    /// Registered skill to compare
    pub skill: String,
    /// Compare only this host, even if it is disabled (default: every enabled
    /// host where the skill is wanted or has a recorded connection)
    #[arg(long)]
    pub host: Option<String>,
    /// Also print a unified diff for changed text files
    #[arg(long)]
    pub patch: bool,
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ResolveArgs {
    /// A registered skill name, or a path to a skill directory or any file
    /// inside one: canonical (skills/<name>/...) or a host connection such as
    /// ~/.claude/skills/<name>/references/setup.md. TARGET is a path when it
    /// is absolute, starts with '.' or '~', or contains a path separator;
    /// otherwise it is a skill name. Exit 0 resolved (including an
    /// unregistered draft under skills/), 1 unresolved or unsafe path,
    /// 2 unknown skill name.
    pub target: String,
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// Skill name: lowercase letters, digits, and hyphens
    pub name: String,
    /// Manifest group for the new skill
    #[arg(long)]
    pub group: Option<String>,
    /// Host the skill targets (repeatable; default: every enabled host)
    #[arg(long = "host", value_name = "HOST")]
    pub hosts: Vec<String>,
    /// Frontmatter description (default: a placeholder to replace)
    #[arg(long)]
    pub description: Option<String>,
    /// Show what would be created; write nothing
    #[arg(long)]
    pub dry_run: bool,
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct HooksArgs {
    #[command(subcommand)]
    pub action: HooksAction,
}

#[derive(Debug, Subcommand)]
pub enum HooksAction {
    /// Write the managed hooks into this registry's .git/hooks
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
    /// Show which hooks would be written; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct HooksUninstallArgs {
    /// Show which hooks would be removed; write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct HooksRunArgs {
    /// pre-commit | post-commit | post-merge | post-rewrite | post-checkout
    pub hook: String,
    /// Arguments git passed to the hook
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub git_args: Vec<String>,
}

/// `set-root` and `init-registry` take the registry path as their argument;
/// `--root` never saves anything, so the two cannot be combined.
pub const ROOT_WITH_SET_ROOT: &str = "--root cannot be combined with set-root or init-registry; pass the registry path as their argument";

impl Command {
    /// True when the user asked for machine-readable output, so errors should
    /// be machine-readable too.
    pub fn wants_json(&self) -> bool {
        match self {
            Command::Validate(args) => args.json,
            Command::Doctor(args) => args.json,
            Command::Install(args) => args.json,
            Command::Share(args) => args.json,
            Command::Status(args) => args.json,
            Command::Skills(args) => args.json,
            Command::Sync(args) => args.json,
            Command::Diff(args) => args.json,
            Command::Resolve(args) => args.json,
            Command::New(args) => args.json,
            _ => false,
        }
    }
}

pub fn run(cli: Cli) -> ExitCode {
    if let Some(command) = &cli.command {
        commands::set_json_errors(command.wants_json());
        if cli.root.is_some() && matches!(command, Command::SetRoot(_) | Command::InitRegistry(_)) {
            return commands::config_error(ROOT_WITH_SET_ROOT);
        }
    }
    crate::registry::root::set_explicit_root(cli.root);
    // A wrong --root is a usage error for every command, including ones that
    // never resolve the registry (update, data-path), and a bare `my-skills`.
    if let Err(error) = crate::registry::root::check_explicit_root() {
        return commands::config_error(error);
    }
    let Some(command) = cli.command else {
        return commands::first_run::bare();
    };
    match command {
        Command::Validate(args) => commands::inspect::validate(&args),
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
        Command::Diff(args) => commands::diff::run(&args),
        Command::Resolve(args) => commands::resolve::run(&args),
        Command::New(args) => commands::registry::new_skill(&args),
    }
}
