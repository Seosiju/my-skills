use std::fs;
use std::io;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use walkdir::WalkDir;

use crate::drive::ignore::is_ignored;

use super::patterns::compile;
use super::rules::finding;
use super::{AuditFinding, AuditPolicy, commands, markdown, static_checks, structure};

type FileAnalyzer = (&'static str, fn(&FileContext, &mut Vec<AuditFinding>));
const FILE_ANALYZERS: [FileAnalyzer; 4] = [
    ("static", static_checks::analyze),
    ("structure", structure::analyze),
    ("markdown", markdown::analyze),
    ("command-tier", commands::analyze),
];

pub(super) struct FileContext {
    pub file: String,
    pub text: Option<String>,
    pub decode_error: bool,
    pub has_nul: bool,
}

fn relative_display(relative: &Path) -> String {
    let text = relative.to_string_lossy();
    if MAIN_SEPARATOR == '/' {
        text.into_owned()
    } else {
        text.replace(MAIN_SEPARATOR, "/")
    }
}

impl FileContext {
    fn read(root: &Path, relative: &Path) -> Self {
        let mut context = Self {
            file: relative_display(relative),
            text: None,
            decode_error: false,
            has_nul: false,
        };
        match fs::read(root.join(relative)) {
            Ok(raw) => {
                context.has_nul = raw.contains(&0);
                if has_text_suffix(relative) {
                    match String::from_utf8(raw) {
                        Ok(text) => context.text = Some(text),
                        Err(_) => context.decode_error = true,
                    }
                }
            }
            Err(error) => {
                context.decode_error = true;
                // Python exposes the read error as text to the other file analyzers.
                context.text = Some(format!(
                    "read error: {}",
                    path_error(&root.join(relative), &error)
                ));
            }
        }
        context
    }
}

pub(super) fn scan(root: &Path, policy: &AuditPolicy) -> Vec<AuditFinding> {
    let mut file_analyzers = FILE_ANALYZERS
        .into_iter()
        .filter(|(id, _)| !policy.is_disabled(id))
        .peekable();
    let dataflow_enabled = !policy.is_disabled("dataflow");
    let mut findings = Vec::new();
    if file_analyzers.peek().is_none() && !dataflow_enabled {
        return findings;
    }

    let files = relative_files(root);
    if file_analyzers.peek().is_some() {
        let contexts: Vec<_> = files
            .iter()
            .map(|file| FileContext::read(root, file))
            .collect();
        for (_, analyze) in file_analyzers {
            for context in &contexts {
                analyze(context, &mut findings);
            }
        }
    }
    if dataflow_enabled {
        match dataflow(root, &files) {
            Ok(flow) => findings.extend(flow),
            Err(error) => findings.push(error),
        }
    }
    findings.retain(|finding| !policy.is_disabled(&finding.rule_id));
    findings
}

fn relative_files(root: &Path) -> Vec<PathBuf> {
    // Pre-order traversal sorted by each component matches sorted(Path.rglob("*")).
    let entries = WalkDir::new(root).min_depth(1).sort_by(|left, right| {
        #[cfg(windows)]
        return left
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .cmp(&right.file_name().to_string_lossy().to_lowercase());
        #[cfg(not(windows))]
        return left.file_name().cmp(right.file_name());
    });
    entries
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("walked path under root");
            if is_ignored(relative) {
                return None;
            }
            let kind = entry.file_type();
            (kind.is_file() || (kind.is_symlink() && entry.path().is_file()))
                .then(|| relative.to_owned())
        })
        .collect()
}

fn has_text_suffix(file: &Path) -> bool {
    file.extension().is_none_or(|suffix| {
        matches!(
            suffix.to_string_lossy().to_ascii_lowercase().as_str(),
            "" | "cfg" | "ini" | "json" | "md" | "py" | "sh" | "toml" | "txt" | "yaml" | "yml"
        )
    })
}

fn dataflow(root: &Path, files: &[PathBuf]) -> Result<Option<AuditFinding>, AuditFinding> {
    let mut readers = Vec::new();
    let mut senders = Vec::new();
    for relative in files.iter().filter(|path| has_text_suffix(path)) {
        let path = root.join(relative);
        let raw = fs::read(&path).map_err(|error| dataflow_read_error(&path, &error))?;
        let Ok(text) = std::str::from_utf8(&raw) else {
            continue;
        };
        // read_text() in Python uses universal newlines. File analyzers decode
        // bytes directly and therefore keep the original line endings.
        let normalized;
        let text = if text.contains('\r') {
            normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            &normalized
        } else {
            text
        };
        let (reader, sender) = commands::capabilities(text);
        if reader {
            readers.push(relative_display(relative));
        }
        if sender {
            senders.push(relative_display(relative));
        }
    }
    if readers.is_empty() || senders.is_empty() {
        return Ok(None);
    }
    Ok(Some(finding(
        "credential-network-flow",
        "<skill>",
        format!(
            "skill bundle combines credential readers ({}) with network senders ({})",
            readers.join(", "),
            senders.join(", "),
        ),
        None,
    )))
}

fn dataflow_read_error(path: &Path, error: &io::Error) -> AuditFinding {
    finding(
        "audit-read-error",
        "SKILL.md",
        format!("dataflow failed: {}", path_error(path, error)),
        None,
    )
}

fn path_error(path: &Path, error: &io::Error) -> String {
    #[cfg(not(windows))]
    let (errno, description) = {
        let number = error.raw_os_error().unwrap_or(5);
        let description = error.to_string();
        let suffix = format!(" (os error {number})");
        let description = description.strip_suffix(&suffix).unwrap_or(&description);
        (number, description.to_owned())
    };
    #[cfg(windows)]
    let (errno, description) = match error.kind() {
        io::ErrorKind::NotFound => (2, "No such file or directory"),
        io::ErrorKind::PermissionDenied => (13, "Permission denied"),
        io::ErrorKind::InvalidInput => (22, "Invalid argument"),
        _ => (5, "Input/output error"),
    };
    format!("[Errno {errno}] {description}: {}", path_repr(path))
}

fn path_repr(path: &Path) -> String {
    use std::fmt::Write;

    static UNPRINTABLE: LazyLock<Regex> = LazyLock::new(|| compile(r"[\p{C}\p{Z}--[ ]]"));
    let path = path.to_string_lossy();
    let quote = if path.contains('\'') && !path.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for ch in path.chars() {
        match ch {
            '\t' => result.push_str(r"\t"),
            '\n' => result.push_str(r"\n"),
            '\r' => result.push_str(r"\r"),
            ch if ch == quote || ch == '\\' => {
                result.push('\\');
                result.push(ch);
            }
            ch if UNPRINTABLE.is_match(ch.encode_utf8(&mut [0; 4])) => {
                let code = ch as u32;
                if code <= 0xff {
                    write!(result, r"\x{code:02x}").unwrap();
                } else if code <= 0xffff {
                    write!(result, r"\u{code:04x}").unwrap();
                } else {
                    write!(result, r"\U{code:08x}").unwrap();
                }
            }
            _ => result.push(ch),
        }
    }
    result.push(quote);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::audit::Severity;

    #[test]
    fn reports_scanner_read_errors_in_python_format() {
        #[cfg(windows)]
        let permission_denied = 5;
        #[cfg(not(windows))]
        let permission_denied = 13;
        let finding = dataflow_read_error(
            Path::new("synthetic's skill file"),
            &io::Error::from_raw_os_error(permission_denied),
        );
        assert_eq!(finding.rule_id, "audit-read-error");
        assert_eq!(finding.category, "scanner");
        assert_eq!(finding.severity, Severity::Critical);
        assert_eq!(finding.file, "SKILL.md");
        assert_eq!(
            finding.message,
            "dataflow failed: [Errno 13] Permission denied: \"synthetic's skill file\""
        );
        assert_eq!(finding.line, None);
    }

    #[test]
    fn missing_text_file_produces_a_scanner_finding() {
        let root = tempfile::tempdir().unwrap();
        let reader = PathBuf::from("reader.txt");
        let sender = PathBuf::from("sender.txt");
        let missing = PathBuf::from("unavailable.txt");
        fs::write(root.path().join(&reader), "op read").unwrap();
        fs::write(root.path().join(&sender), "curl https://example.invalid/").unwrap();
        let finding = dataflow(root.path(), &[reader, sender, missing]).unwrap_err();
        assert_eq!(finding.rule_id, "audit-read-error");
        assert!(
            finding
                .message
                .starts_with("dataflow failed: [Errno 2] No such file or directory: ")
        );
        assert!(
            finding
                .message
                .ends_with(&path_repr(&root.path().join("unavailable.txt")))
        );
    }

    #[test]
    fn relative_display_normalizes_separators_to_forward_slash() {
        let nested = Path::new("nested").join("sub").join("sender.sh");
        assert_eq!(relative_display(&nested), "nested/sub/sender.sh");
    }
}
