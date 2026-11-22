//! Command-line surface. Owned by the integrator.
//!
//! Commands keep v0.4.1 names and flags (spec §6.9). `convert` and `hooks`
//! are new (spec §6.6, §6.7). Until a command is wired to its module it
//! exits with code 2 and says so, so a v1 alpha is never mistaken for a
//! working command.

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

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

#[derive(Debug, Args)]
pub struct Selection {
    /// Skill name (default: enabled skills)
    pub skill: Option<String>,
    /// Target host name, or 'all' (default: enabled targets)
    #[arg(long)]
    pub host: Option<String>,
    /// Include every registered skill
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate canonical skills against the Agent Skills standard
    Validate {
        skill: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Audit canonical skills for agent-skill security risks
    Audit {
        skill: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Report environment, hosts, registry, hooks, and manifest health
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Update the installed my-skills binary
    Update {
        #[arg(long)]
        check: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Connect enabled skills for this machine
    Bootstrap {
        #[arg(long)]
        dry_run: bool,
    },
    /// Create a private canonical skill registry scaffold
    InitRegistry { path: Option<String> },
    /// Set the active registry root used when running outside a registry
    SetRoot { path: String },
    /// Connect skills into host directories (link by default)
    Install {
        #[command(flatten)]
        selection: Selection,
        #[arg(long, value_enum)]
        mode: Option<ModeArg>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        skip_audit: bool,
    },
    /// Reconcile host directories with the manifest (create and prune)
    Sync {
        #[command(flatten)]
        selection: Selection,
        /// Report drift only; change nothing
        #[arg(long)]
        check: bool,
        /// Do not remove connections the manifest no longer wants
        #[arg(long)]
        no_prune: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        quiet: bool,
        #[arg(long)]
        skip_audit: bool,
    },
    /// Remove managed connections (never the canonical source)
    Uninstall {
        skill: String,
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Switch existing connections between link and copy
    Convert {
        #[command(flatten)]
        selection: Selection,
        #[arg(long, value_enum)]
        to: ModeArg,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Install, remove, or inspect the registry's git hooks
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
    /// Show connection status per skill and host
    Status {
        #[arg(long)]
        json: bool,
    },
    /// List registered canonical skills
    Skills {
        #[arg(long)]
        json: bool,
    },
    /// Plan or perform sharing host-local skills into canonical
    Share {
        #[arg(long)]
        from: String,
        skill: Option<String>,
        #[arg(long)]
        plan: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        enable: bool,
    },
    /// Enable a registered skill in the manifest
    Enable { skill: String },
    /// Disable a registered skill in the manifest
    Disable { skill: String },
    /// Import an external skill directory into canonical skills/
    Import {
        source: String,
        #[arg(long)]
        enable: bool,
    },
    /// Print a skill's machine-local data directory
    DataPath {
        skill: String,
        #[arg(long)]
        create: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum HooksAction {
    Install,
    Uninstall,
    Status,
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Validate { .. } => "validate",
        Command::Audit { .. } => "audit",
        Command::Doctor { .. } => "doctor",
        Command::Update { .. } => "update",
        Command::Bootstrap { .. } => "bootstrap",
        Command::InitRegistry { .. } => "init-registry",
        Command::SetRoot { .. } => "set-root",
        Command::Install { .. } => "install",
        Command::Sync { .. } => "sync",
        Command::Uninstall { .. } => "uninstall",
        Command::Convert { .. } => "convert",
        Command::Hooks { .. } => "hooks",
        Command::Status { .. } => "status",
        Command::Skills { .. } => "skills",
        Command::Share { .. } => "share",
        Command::Enable { .. } => "enable",
        Command::Disable { .. } => "disable",
        Command::Import { .. } => "import",
        Command::DataPath { .. } => "data-path",
    }
}

pub fn run(cli: Cli) -> ExitCode {
    let name = command_name(&cli.command);
    eprintln!("my-skills {name}: not implemented yet in this v1 alpha build");
    ExitCode::from(2)
}
