//! Command-line surface. Owned by the integrator.
//!
//! v1.5.0 surface (15 subcommands): `init-registry`, `bootstrap`, `set-root`,
//! `update`, `sync`, `skills`, `enable`, `disable`, `new`, `import`,
//! `validate`, `diff`, `doctor`, `hooks`, and `data-path`. Each command's
//! handler lives in `crate::commands`.

use std::ffi::OsString;
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
`sync --dry-run` exits 1 only on validation failure; read `blocked` and
`applicable` in its --json output, or use `sync --check`, which exits 1 when
anything is out of date."
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
    /// Connect enabled skills and install git hooks for this machine
    Bootstrap(BootstrapArgs),
    /// Create a private canonical skill registry scaffold
    InitRegistry(InitRegistryArgs),
    /// Set the active registry root used when running outside a registry
    SetRoot(SetRootArgs),
    /// Enable registered skills in the manifest
    Enable(SkillNameArgs),
    /// Disable registered skills in the manifest
    Disable(SkillNameArgs),
    /// List registered canonical skills and their per-host connection status
    Skills(SkillsArgs),
    /// Reconcile host directories with the manifest (connect, convert, and prune)
    Sync(SyncArgs),
    /// Import an external skill directory or a host-local skill into canonical skills/
    Import(ImportArgs),
    /// Print a skill's shared machine-local data directory
    DataPath(DataPathArgs),
    /// Install, remove, or inspect the registry's git hooks
    Hooks(HooksArgs),
    /// Show how a host connection differs from its canonical skill (read-only)
    Diff(DiffArgs),
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
    /// Skip connecting skills
    #[arg(long)]
    pub skip_skill_install: bool,
    /// Connect only to this host (default: every enabled host)
    #[arg(long)]
    pub host: Option<String>,
    /// Also connect disabled skills
    #[arg(long)]
    pub all: bool,
    /// Install mode for this run (new connections and mode conversions); later
    /// bare sync and hooks follow the manifest
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
pub struct SkillsArgs {
    /// Only these skills (default: every registered skill); combines with
    /// the filters below as AND
    pub skills: Vec<String>,
    /// Machine-readable output (per-host connection status and details included)
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
    /// error when present); the table is unchanged
    #[arg(long)]
    pub brief: bool,
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
    /// Install mode for this run (new connections and mode conversions); later
    /// bare sync and hooks follow the manifest
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
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
pub struct ImportArgs {
    /// Skill directory path (without --from) or host-local skill name (with --from)
    pub target: Option<String>,
    /// Import a host-local skill from this host instead of a directory path
    #[arg(long = "from", value_name = "HOST")]
    pub from_host: Option<String>,
    /// With --from: list host-local skills that could be imported; write nothing
    #[arg(long)]
    pub plan: bool,
    /// Overwrite a different canonical skill with the same name
    #[arg(long)]
    pub force: bool,
    /// Register the imported skill as enabled
    #[arg(long, conflicts_with = "disable")]
    pub enable: bool,
    /// Register the imported skill as disabled
    #[arg(long)]
    pub disable: bool,
    /// Show what would change; write nothing
    #[arg(long)]
    pub dry_run: bool,
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
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

/// If argv invokes one of the six subcommands removed in v1.5.0, return a
/// migration error message and hint so callers get actionable guidance with
/// exit code 2 (without acting as an executable alias).
pub fn removed_command_hint(raw_args: &[OsString]) -> Option<(&'static str, &'static str)> {
    let mut index = 1;
    while index < raw_args.len() {
        let arg = raw_args[index].to_str()?;
        if arg == "--root" {
            index += 2;
            continue;
        }
        if arg.starts_with("--root=") {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            return None;
        }
        return match arg {
            "install" => Some((
                "'install' was removed in v1.5.0; use 'my-skills sync'",
                "Use 'my-skills sync' (add --no-prune to skip removals).",
            )),
            "uninstall" => Some((
                "'uninstall' was removed in v1.5.0; use 'my-skills disable <skill>' followed by 'my-skills sync'",
                "Run 'my-skills disable <skill>' and then 'my-skills sync'.",
            )),
            "convert" => Some((
                "'convert' was removed in v1.5.0; set install_mode in my-skills.toml (or my-skills.local.toml) and run 'my-skills sync', or pass 'my-skills sync --mode <link|copy>'",
                "Set install_mode in my-skills.toml or my-skills.local.toml and run 'my-skills sync'.",
            )),
            "status" => Some((
                "'status' was removed in v1.5.0; use 'my-skills skills'",
                "Use 'my-skills skills' (or 'my-skills skills --json').",
            )),
            "share" => Some((
                "'share' was removed in v1.5.0; use 'my-skills import --from <host>'",
                "Use 'my-skills import --from <host> --plan' or 'my-skills import --from <host> <skill> --enable|--disable'.",
            )),
            "resolve" => Some((
                "'resolve' was removed in v1.5.0; use 'my-skills skills <name> --json' for canonical_path",
                "Use 'my-skills skills <name> --json' to read canonical_path.",
            )),
            _ => None,
        };
    }
    None
}

impl Command {
    /// True when the user asked for machine-readable output, so errors should
    /// be machine-readable too.
    pub fn wants_json(&self) -> bool {
        match self {
            Command::Validate(args) => args.json,
            Command::Doctor(args) => args.json,
            Command::Skills(args) => args.json,
            Command::Sync(args) => args.json,
            Command::Import(args) => args.json,
            Command::Diff(args) => args.json,
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
        Command::Enable(args) => commands::registry::enable(&args),
        Command::Disable(args) => commands::registry::disable(&args),
        Command::Skills(args) => commands::registry::skills(&args),
        Command::Sync(args) => commands::connect::sync(&args),
        Command::Import(args) => commands::registry::import(&args),
        Command::DataPath(args) => commands::inspect::data_path(&args),
        Command::Hooks(args) => commands::hooks::run(&args),
        Command::Diff(args) => commands::diff::run(&args),
        Command::New(args) => commands::registry::new_skill(&args),
    }
}
