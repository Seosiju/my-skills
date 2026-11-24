//! Read-only comparison of a canonical skill with its host connections.

use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use super::config_error;
use crate::cli::DiffArgs;
use crate::drive::diff::{self, FileChanges};
use crate::drive::link::{self, EntryKind};
use crate::drive::{plan, state::State};
use crate::platform::paths::display_path;
use crate::registry::manifest::{CliOverrides, InstallMode, Manifest, Skill, select_names};
use crate::registry::root::{self, RegistryMeta};
use crate::{Error, Result};

#[derive(Serialize)]
struct Report<'a> {
    registry: RegistryMeta,
    skill: &'a str,
    different: bool,
    hosts: Vec<HostDiff<'a>>,
}

#[derive(Serialize)]
struct HostDiff<'a> {
    host: &'a str,
    status: &'static str,
    mode: Option<InstallMode>,
    destination: String,
    /// The owning skills root recorded by the drive for a foreign entry.
    owner: Option<String>,
    files: FileChanges,
    patch: Option<String>,
    /// Null for any entry that is not a link.
    link_matches: Option<bool>,
}

pub fn run(args: &DiffArgs) -> ExitCode {
    let (manifest, resolution) = match root::load_manifest_resolved(&CliOverrides::default(), false)
    {
        Ok(loaded) => loaded,
        Err(error) => return config_error(error),
    };
    let report = match compare(&manifest, resolution.meta(), args) {
        Ok(report) => report,
        Err(error) => return config_error(error),
    };
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("diff report is serializable")
        );
    } else {
        report.print();
    }
    ExitCode::from(u8::from(report.different))
}

fn compare<'a>(
    manifest: &'a Manifest,
    registry: RegistryMeta,
    args: &DiffArgs,
) -> Result<Report<'a>> {
    let names = select_names(manifest, std::slice::from_ref(&args.skill), &[])?;
    let skill = &manifest.skills[&names[0]];
    let selected = args.host.as_deref();
    if let Some(host) = selected.filter(|host| !manifest.targets.contains_key(*host)) {
        return Err(Error::Manifest(format!("unknown host: {host}")));
    }
    let state = State::load(None)?;
    let mut report = Report {
        registry,
        skill: &skill.name,
        different: false,
        hosts: Vec::new(),
    };
    for target in manifest.targets.values() {
        let host = target.name.as_str();
        if !target.enabled || selected.is_some_and(|name| name != host) {
            continue;
        }
        if !plan::wants(manifest, skill, host) && state.get(&skill.name, host).is_none() {
            continue;
        }
        let (row, different) = compare_host(manifest, skill, host, &state, args.patch)?;
        report.hosts.push(row);
        report.different |= different;
    }
    Ok(report)
}

fn compare_host<'a>(
    manifest: &Manifest,
    skill: &Skill,
    host: &'a str,
    state: &State,
    include_patch: bool,
) -> Result<(HostDiff<'a>, bool)> {
    let status = plan::status_of(manifest, skill, host, state)?;
    let record = state.get(&skill.name, host);
    let source = manifest.skill_dir(&skill.name);
    let dest = plan::destination(manifest, host, &skill.name)?;
    let owner = record
        .filter(|_| status == plan::Status::Foreign)
        .and_then(|record| Path::new(&record.source).parent())
        .map(display_path);
    let mut row = HostDiff {
        host,
        status: status.label(),
        mode: record.map(|record| record.mode),
        destination: display_path(&dest),
        owner,
        files: FileChanges::default(),
        patch: None,
        link_matches: None,
    };
    // Status tracks history since the last sync. The comparison uses the
    // current files and detects a recorded connection replaced by another type.
    let different = match link::entry_kind(&dest)? {
        EntryKind::Link(_) => {
            row.mode = Some(InstallMode::Link);
            let matches = link::is_managed_link(&dest, &source);
            row.link_matches = Some(matches);
            !matches || !source.is_dir() || status == plan::Status::Drifted
        }
        EntryKind::Dir => {
            row.mode = Some(InstallMode::Copy);
            (row.files, row.patch) = diff::compare(&source, &dest, include_patch)?;
            !source.is_dir()
                || !row.files.is_empty()
                || (status == plan::Status::Drifted
                    && record.is_some_and(|record| record.mode == InstallMode::Link))
        }
        EntryKind::Missing | EntryKind::File => true,
    };
    Ok((row, different))
}

impl Report<'_> {
    fn print(&self) {
        println!("{}", self.skill);
        if self.hosts.is_empty() {
            println!("  no hosts to compare");
        }
        for row in &self.hosts {
            let mode = match row.mode {
                Some(InstallMode::Link) => "link",
                Some(InstallMode::Copy) => "copy",
                None => "unknown mode",
            };
            println!(
                "  {:8} {} ({mode}) {}",
                row.host, row.status, row.destination
            );
            if let Some(owner) = &row.owner {
                println!("    owner: {owner}");
            }
            match row.link_matches {
                Some(true) => println!("    link points to the canonical skill"),
                Some(false) => println!("    link points elsewhere"),
                None => {}
            }
            for (label, names) in [
                ("added", &row.files.added),
                ("removed", &row.files.removed),
                ("modified", &row.files.modified),
            ] {
                for name in names {
                    println!("    {label}: {name}");
                }
            }
            if let Some(patch) = &row.patch {
                print!("{patch}");
            }
        }
    }
}
