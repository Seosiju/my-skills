//! Skill listing and metadata (v0.4.1 catalog.py). Stream H.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::Result;
use crate::check::audit::{self, AuditPolicy, Severity};
use crate::check::frontmatter::{Mapping, Node, parse_document};
use crate::drive::plan::{status_of, wants};
use crate::drive::state::State;
use crate::registry::manifest::{Manifest, Skill};

#[derive(Serialize)]
pub(crate) struct Catalog {
    skills: Vec<Row>,
}

#[derive(Serialize)]
struct Row {
    #[serde(flatten)]
    summary: Summary,
    description: String,
    path: String,
    canonical_path: String,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    governance: Option<Governance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct Summary {
    name: String,
    enabled: bool,
    hosts: Vec<String>,
    group: Option<String>,
    status: BTreeMap<String, &'static str>,
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
    selected: Option<&[String]>,
    with_governance: bool,
) -> Result<Catalog> {
    let policy = with_governance
        .then(|| audit::policy_from_settings(&manifest.audit))
        .transpose()?;
    let mut skills = Vec::new();
    for skill in manifest.skills.values() {
        if enabled.is_some_and(|enabled| enabled != skill.enabled)
            || host.is_some_and(|host| !supports(&skill.hosts, host))
            || selected.is_some_and(|names| names.binary_search(&skill.name).is_err())
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
        let directory = manifest.skill_dir(&skill.name);
        let (description, error) = match metadata(&directory) {
            Ok(meta) => (
                field(&meta, "description").map_or_else(String::new, Node::python_string),
                None,
            ),
            Err(err) => (
                String::new(),
                Some(format!("Could not read skill metadata: {err}")),
            ),
        };
        skills.push(Row {
            summary: Summary {
                name: skill.name.clone(),
                enabled: skill.enabled,
                hosts: skill.hosts.clone(),
                group: skill.group.clone(),
                status,
            },
            description,
            path: display_path(&Path::new(&manifest.skills_root).join(&skill.name)),
            canonical_path: display_path(&directory),
            governance,
            error,
        });
    }
    Ok(Catalog { skills })
}

#[derive(Default, Serialize)]
pub(crate) struct CatalogCost {
    pub(crate) skills: usize,
    pub(crate) description_chars: usize,
    pub(crate) longest: Vec<DescriptionLength>,
    #[serde(rename = "invalid")]
    pub(crate) without_description: usize,
}

#[derive(Serialize)]
pub(crate) struct DescriptionLength {
    pub(crate) name: String,
    pub(crate) chars: usize,
}

pub(crate) fn costs(
    manifest: &Manifest,
    hosts: impl IntoIterator<Item = &'static str>,
) -> BTreeMap<&'static str, CatalogCost> {
    let mut costs = hosts
        .into_iter()
        .filter(|host| {
            manifest
                .targets
                .get(*host)
                .is_some_and(|target| target.enabled)
        })
        .map(|host| (host, CatalogCost::default()))
        .collect::<BTreeMap<_, _>>();
    for skill in manifest.skills.values() {
        let mut targets = costs
            .iter_mut()
            .filter(|(host, _)| wants(manifest, skill, host))
            .peekable();
        if targets.peek().is_none() {
            continue;
        }
        let description_chars = metadata(&manifest.skill_dir(&skill.name))
            .ok()
            .and_then(|meta| match field(&meta, "description") {
                Some(Node::String(text)) if !text.trim().is_empty() => Some(text.chars().count()),
                _ => None,
            });
        for (_, cost) in targets {
            cost.skills += 1;
            if let Some(chars) = description_chars {
                cost.description_chars += chars;
                cost.longest.push(DescriptionLength {
                    name: skill.name.clone(),
                    chars,
                });
                cost.longest
                    .sort_by(|a, b| b.chars.cmp(&a.chars).then_with(|| a.name.cmp(&b.name)));
                cost.longest.truncate(2);
            } else {
                cost.without_description += 1;
            }
        }
    }
    costs
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
    pub(crate) fn brief(&self) -> impl Serialize + '_ {
        #[derive(Serialize)]
        struct BriefRow<'a> {
            #[serde(flatten)]
            summary: &'a Summary,
            #[serde(skip_serializing_if = "Option::is_none")]
            governance: Option<&'a Governance>,
            #[serde(skip_serializing_if = "Option::is_none")]
            error: Option<&'a str>,
        }

        #[derive(Serialize)]
        struct BriefCatalog<'a> {
            skills: Vec<BriefRow<'a>>,
        }

        BriefCatalog {
            skills: self
                .skills
                .iter()
                .map(|row| BriefRow {
                    summary: &row.summary,
                    governance: row.governance.as_ref(),
                    error: row.error.as_deref(),
                })
                .collect(),
        }
    }

    pub(crate) fn table(&self, status_hosts: &[String]) -> String {
        let include_governance = self.skills.iter().any(|row| row.governance.is_some());
        let mut headers = vec!["SKILL".into(), "ENABLED".into()];
        headers.extend(status_hosts.iter().map(|host| host.to_uppercase()));
        if include_governance {
            headers.extend(["AUDIT".into(), "TRUST".into()]);
        }
        let rows = self.skills.iter().map(|row| {
            let summary = &row.summary;
            let name = if row.description.trim().is_empty() {
                format!("{} (no description)", summary.name)
            } else {
                summary.name.clone()
            };
            let mut cells = vec![name, if summary.enabled { "yes" } else { "no" }.into()];
            cells.extend(status_hosts.iter().map(|host| {
                if supports(&summary.hosts, host) {
                    summary.status.get(host).unwrap_or(&"-").to_lowercase()
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
    crate::platform::paths::display_path(path)
}

pub(crate) fn json_output(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("registry output is serializable")
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
