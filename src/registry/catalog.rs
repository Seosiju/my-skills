//! Skill listing and metadata (v0.4.1 catalog.py). Stream H.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::Result;
use crate::check::audit::{self, AuditPolicy, Severity};
use crate::check::frontmatter::{Mapping, Node, parse_document};
use crate::drive::plan::status_of;
use crate::drive::state::State;
use crate::registry::manifest::{Manifest, Skill};

#[derive(Serialize)]
pub(crate) struct Catalog {
    skills: Vec<Row>,
}

#[derive(Serialize)]
struct Row {
    name: String,
    enabled: bool,
    hosts: Vec<String>,
    description: String,
    path: String,
    status: BTreeMap<String, &'static str>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    governance: Option<Governance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct Governance {
    audit: Audit,
    provenance: Provenance,
}

#[derive(Serialize)]
struct Audit {
    status: &'static str,
    profile: String,
    threshold: &'static str,
    result_hash: String,
    findings: String,
}

#[derive(Serialize)]
struct Provenance {
    source_type: String,
    trust_tier: &'static str,
    last_audit_result_hash: String,
}

pub(crate) fn rows(
    manifest: &Manifest,
    state: &State,
    host: Option<&str>,
    enabled: Option<bool>,
    status_hosts: &[String],
    with_status: bool,
) -> Result<Catalog> {
    let policy = with_status
        .then(|| audit::policy_from_settings(&manifest.audit))
        .transpose()?;
    let mut skills = Vec::new();
    for skill in manifest.skills.values() {
        if enabled.is_some_and(|enabled| enabled != skill.enabled)
            || host.is_some_and(|host| !supports(&skill.hosts, host))
        {
            continue;
        }
        let status = status_hosts
            .iter()
            .map(|host| {
                Ok((
                    host.clone(),
                    status_of(manifest, skill, host, state)?.label(),
                ))
            })
            .collect::<Result<_>>()?;
        let governance = policy
            .as_ref()
            .map(|policy| governance(manifest, skill, state, policy))
            .transpose()?;
        let (description, error) = match metadata(&manifest.skill_dir(&skill.name)) {
            Ok(meta) => (
                field(&meta, "description").map_or_else(String::new, Node::python_string),
                None,
            ),
            Err(err) => {
                let error = format!("invalid skill metadata for {}: {err}", skill.name);
                (format!("invalid: {error}"), Some(error))
            }
        };
        skills.push(Row {
            name: skill.name.clone(),
            enabled: skill.enabled,
            hosts: skill.hosts.clone(),
            description,
            path: display_path(&Path::new(&manifest.skills_root).join(&skill.name)),
            status,
            governance,
            error,
        });
    }
    Ok(Catalog { skills })
}

fn supports(hosts: &[String], host: &str) -> bool {
    hosts.is_empty() || hosts.iter().any(|candidate| candidate == host)
}

fn governance(
    manifest: &Manifest,
    skill: &Skill,
    state: &State,
    policy: &AuditPolicy,
) -> Result<Governance> {
    let result = audit::run_audit(&manifest.skill_dir(&skill.name), policy)?;
    let records = || {
        state.records().filter(|record| {
            record.skill == skill.name && !record.last_audit_result_hash.is_empty()
        })
    };
    let record = records()
        .find(|record| record.source_type == "host")
        .or_else(|| records().next());
    let result_hash = result.result_hash();
    let source_type = record.map_or("canonical", |item| item.source_type.as_str());
    Ok(Governance {
        audit: Audit {
            status: if result.blocked() { "blocked" } else { "ok" },
            threshold: result.threshold.map_or("none", Severity::label),
            profile: result.profile,
            findings: result.findings.len().to_string(),
            result_hash: result_hash.clone(),
        },
        provenance: Provenance {
            source_type: source_type.into(),
            trust_tier: if source_type == "host" {
                "imported-local"
            } else {
                "local-authored"
            },
            last_audit_result_hash: record
                .map_or(result_hash, |item| item.last_audit_result_hash.clone()),
        },
    })
}

impl Catalog {
    pub(crate) fn table(&self, status_hosts: &[String]) -> String {
        let include_governance = self.skills.iter().any(|row| row.governance.is_some());
        let mut headers = vec!["SKILL".into(), "ENABLED".into()];
        headers.extend(status_hosts.iter().map(|host| host.to_uppercase()));
        if include_governance {
            headers.extend(["AUDIT".into(), "TRUST".into()]);
        }
        let rows = self.skills.iter().map(|row| {
            let name = if row.error.is_some() {
                format!("{} (invalid)", row.name)
            } else {
                row.name.clone()
            };
            let mut cells = vec![name, if row.enabled { "yes" } else { "no" }.into()];
            cells.extend(status_hosts.iter().map(|host| {
                if supports(&row.hosts, host) {
                    row.status.get(host).unwrap_or(&"-").to_lowercase()
                } else {
                    "-".into()
                }
            }));
            if include_governance {
                let (audit, trust) = row.governance.as_ref().map_or(("-", "-"), |info| {
                    (info.audit.status, info.provenance.trust_tier)
                });
                cells.extend([audit.into(), trust.into()]);
            }
            cells
        });
        table(headers, rows, true)
    }
}

pub(crate) fn metadata(path: &Path) -> std::result::Result<Mapping, String> {
    let skill_md = path.join("SKILL.md");
    let text = fs::read_to_string(&skill_md).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            let path = display_path(&skill_md);
            let repr = if path.contains('\'') && !path.contains('"') {
                format!(
                    "\"{}\"",
                    path.escape_debug().to_string().replace("\\'", "'")
                )
            } else {
                format!(
                    "'{}'",
                    path.escape_debug().to_string().replace("\\\"", "\"")
                )
            };
            format!("[Errno 2] No such file or directory: {repr}")
        } else {
            format!("{}: {err}", display_path(&skill_md))
        }
    })?;
    parse_document(&text)
        .map(|(meta, _)| meta)
        .map_err(|err| err.to_string())
}

pub(crate) fn field<'a>(meta: &'a Mapping, field: &str) -> Option<&'a Node> {
    meta.iter().find_map(|(key, value)| {
        matches!(&**key, Node::String(key) if key == field).then_some(value.as_ref())
    })
}

/// Keep Windows verbatim prefixes for filesystem calls and omit them in
/// user-facing paths, as pathlib does for normally spelled absolute paths.
pub(crate) fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(disk) = text.strip_prefix(r"\\?\") {
            return disk.into();
        }
    }
    text.into_owned()
}

pub(crate) fn json_output(value: &impl Serialize) -> String {
    let json = serde_json::to_string_pretty(value).expect("registry output is serializable");
    let mut ascii = String::with_capacity(json.len());
    for ch in json.chars() {
        if ch <= '\x7e' {
            ascii.push(ch);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]) {
                write!(ascii, "\\u{unit:04x}").unwrap();
            }
        }
    }
    ascii
}

pub(crate) fn table(
    headers: Vec<String>,
    rows: impl IntoIterator<Item = Vec<String>>,
    trim: bool,
) -> String {
    let rows: Vec<_> = rows.into_iter().collect();
    let widths: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(idx, header)| {
            rows.iter()
                .map(|row| row[idx].chars().count())
                .fold(header.chars().count(), usize::max)
        })
        .collect();
    let divider: Vec<_> = widths.iter().map(|width| "-".repeat(*width)).collect();
    [headers, divider]
        .into_iter()
        .chain(rows)
        .map(|row| {
            let line = row
                .into_iter()
                .zip(&widths)
                .map(|(item, width)| format!("{item:width$}"))
                .collect::<Vec<_>>()
                .join("  ");
            if trim {
                line.trim_end().to_owned()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
