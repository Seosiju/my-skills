//! Managed registry git hooks and validation of the content selected for commit.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fmt};

use serde::Serialize;

use crate::check::validate;
use crate::platform::paths;
use crate::registry::manifest::{self, MANIFEST_FILE, Manifest};

const NAMES: [&str; 4] = ["pre-commit", "post-merge", "post-rewrite", "post-checkout"];
const MARKER: &str = "# managed-by: my-skills hooks ";
const VERSION: &str = "v1";

/// Git commands scoped to the registry. Keep the caller's index environment:
/// Git can supply an alternate index to pre-commit, e.g. for a partial commit.
pub(crate) struct Git<'a> {
    root: &'a Path,
}

impl<'a> Git<'a> {
    pub(crate) fn discover(root: &'a Path) -> anyhow::Result<Self> {
        let git = Self { root };
        match git.text(&["rev-parse", "--is-inside-work-tree"]) {
            Ok(answer) if answer == "true" => Ok(git),
            Ok(_) => anyhow::bail!("{} is not a Git worktree", paths::display_path(root)),
            Err(error) => {
                anyhow::bail!(
                    "{} is not a Git worktree: {error}",
                    paths::display_path(root)
                )
            }
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(self.root)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .arg("--literal-pathspecs");
        command
    }

    pub(crate) fn run(&self, args: &[&str]) -> anyhow::Result<Vec<u8>> {
        let output = self.command().args(args).output()?;
        anyhow::ensure!(
            output.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(output.stdout)
    }

    pub(crate) fn text(&self, args: &[&str]) -> anyhow::Result<String> {
        Ok(std::str::from_utf8(output_line(&self.run(args)?))?.to_owned())
    }

    pub(crate) fn path(&self, name: &str) -> anyhow::Result<PathBuf> {
        let output = self.run(&["rev-parse", "--path-format=absolute", "--git-path", name])?;
        Ok(git_path(output_line(&output))?.into())
    }

    fn registry_prefix(&self) -> anyhow::Result<OsString> {
        let prefix = self.run(&["rev-parse", "--show-prefix"])?;
        git_path(output_line(&prefix))
    }
}

fn output_line(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if cfg!(windows) {
        bytes.strip_suffix(b"\r").unwrap_or(bytes)
    } else {
        bytes
    }
}

#[cfg(unix)]
fn git_path(bytes: &[u8]) -> anyhow::Result<OsString> {
    use std::os::unix::ffi::OsStrExt;
    Ok(std::ffi::OsStr::from_bytes(bytes).to_owned())
}

#[cfg(not(unix))]
fn git_path(bytes: &[u8]) -> anyhow::Result<OsString> {
    Ok(std::str::from_utf8(bytes)?.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum HookState {
    Installed,
    Missing,
    Foreign,
}

impl fmt::Display for HookState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
            Self::Foreign => "foreign",
        })
    }
}

#[derive(Serialize)]
struct HookEntry {
    name: &'static str,
    status: HookState,
    version: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct HookStatus {
    #[serde(serialize_with = "serialize_hook_path")]
    path: PathBuf,
    on_path: bool,
    entries: Vec<HookEntry>,
}

impl HookStatus {
    pub(crate) fn print(&self) {
        println!("  Directory: {}", display_hook_path(&self.path));
        println!(
            "  my-skills on PATH: {}",
            if self.on_path { "yes" } else { "no" }
        );
        println!("  {:15} {:10} Version", "Hook", "Status");
        for hook in &self.entries {
            println!(
                "  {:15} {:10} {}",
                hook.name,
                hook.status,
                hook.version.as_deref().unwrap_or("-")
            );
        }
    }
}

fn serialize_hook_path<S: serde::Serializer>(
    path: &Path,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&display_hook_path(path))
}

fn display_hook_path(path: &Path) -> String {
    let displayed = paths::display_path(path);
    #[cfg(windows)]
    {
        Path::new(&displayed)
            .components()
            .collect::<PathBuf>()
            .display()
            .to_string()
    }
    #[cfg(not(windows))]
    {
        displayed
    }
}

fn inspect(path: &Path, name: &'static str) -> io::Result<HookEntry> {
    let (status, version) = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let mut file = BufReader::new(File::open(path)?);
            let mut line = Vec::new();
            file.read_until(b'\n', &mut line)?;
            line.clear();
            file.read_until(b'\n', &mut line)?;
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let version = std::str::from_utf8(line)
                .ok()
                .and_then(|line| line.strip_prefix(MARKER))
                .filter(|version| {
                    version.strip_prefix('v').is_some_and(|digits| {
                        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                    })
                });
            match version {
                Some(version) => (HookState::Installed, Some(version.to_owned())),
                None => (HookState::Foreign, None),
            }
        }
        Ok(_) => (HookState::Foreign, None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (HookState::Missing, None),
        Err(error) => return Err(error),
    };
    Ok(HookEntry {
        name,
        status,
        version,
    })
}

fn inspect_hook_error(file: &Path, error: io::Error) -> crate::Error {
    crate::Error::io(PathBuf::from(display_hook_path(file)), error)
}

pub(crate) fn status(git: &Git<'_>) -> anyhow::Result<HookStatus> {
    let path = git.path("hooks")?;
    let entries = NAMES
        .into_iter()
        .map(|name| {
            let file = path.join(name);
            inspect(&file, name).map_err(|error| inspect_hook_error(&file, error))
        })
        .collect::<Result<_, _>>()?;
    Ok(HookStatus {
        path,
        on_path: command_on_path("my-skills"),
        entries,
    })
}

pub(crate) fn install(git: &Git<'_>, dry_run: bool) -> anyhow::Result<()> {
    let status = status(git)?;
    let mut foreign = false;
    for hook in &status.entries {
        if hook.status == HookState::Foreign {
            foreign = true;
            eprintln!(
                "Foreign hook at {}. Add this line to that hook:",
                display_hook_path(&status.path.join(hook.name))
            );
            eprintln!("  my-skills hooks run {} \"$@\"", hook.name);
        }
    }
    anyhow::ensure!(!foreign, "existing foreign hooks were left in place");

    let executable = shell_path(&env::current_exe()?)?;
    let registry = shell_path(&Path::new(".").join(git.registry_prefix()?))?;
    if !dry_run {
        fs::create_dir_all(&status.path)?;
    }
    for hook in status.entries {
        let path = status.path.join(hook.name);
        if dry_run {
            println!("would install: {}", display_hook_path(&path));
            continue;
        }
        let script = format!(
            "#!/bin/sh\n{MARKER}{VERSION}\n\
             if ! MY_SKILLS=$(command -v my-skills); then\n\
                 MY_SKILLS={executable}\n\
             fi\n\
             export MY_SKILLS_ROOT={registry}\n\
             exec \"$MY_SKILLS\" hooks run {} \"$@\"\n",
            hook.name
        );
        write_hook(&path, hook, &script)?;
        println!("installed: {}", display_hook_path(&path));
    }
    Ok(())
}

fn shell_path(path: &Path) -> anyhow::Result<String> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path is not UTF-8: {}", path.display()))?;
    #[cfg(windows)]
    let path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!("//{}", unc.replace('\\', "/"))
    } else {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('\\', "/")
    };
    Ok(format!("'{}'", path.replace('\'', "'\"'\"'")))
}

fn write_hook(path: &Path, hook: HookEntry, script: &str) -> anyhow::Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().expect("hook has parent"))?;
    temp.write_all(script.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    if hook.status == HookState::Missing {
        temp.persist_noclobber(path)?;
    } else {
        anyhow::ensure!(
            inspect(path, hook.name)?.status == HookState::Installed,
            "hook changed during install: {}",
            display_hook_path(path)
        );
        temp.persist(path)?;
    }
    Ok(())
}

pub(crate) fn uninstall(git: &Git<'_>, dry_run: bool) -> anyhow::Result<()> {
    let status = status(git)?;
    for hook in status.entries {
        let path = status.path.join(hook.name);
        if hook.status == HookState::Installed
            && inspect(&path, hook.name)?.status == HookState::Installed
        {
            if dry_run {
                println!("would remove: {}", display_hook_path(&path));
            } else {
                fs::remove_file(&path)?;
                println!("removed: {}", display_hook_path(&path));
            }
        }
    }
    Ok(())
}

pub(crate) fn command_on_path(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| is_command(&dir, name))
}

#[cfg(unix)]
fn is_command(dir: &Path, name: &str) -> bool {
    let path = dir.join(name);
    path.is_file() && unix_test("-x", &path)
}

#[cfg(windows)]
fn is_command(dir: &Path, name: &str) -> bool {
    let extensions = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    dir.join(name).is_file()
        || extensions
            .split(';')
            .filter(|ext| !ext.is_empty())
            .any(|ext| dir.join(format!("{name}{ext}")).is_file())
}

#[cfg(not(any(unix, windows)))]
fn is_command(dir: &Path, name: &str) -> bool {
    dir.join(name).is_file()
}

#[cfg(unix)]
pub(crate) fn unix_test(flag: &str, path: &Path) -> bool {
    Command::new("/bin/sh")
        .args(["-c", "test \"$1\" \"$2\"", "sh", flag])
        .arg(path)
        .status()
        .is_ok_and(|status| status.success())
}

/// A regular file and its exact staged blob. Links and gitlinks could lead
/// validation outside the index snapshot and have no portable skill content.
struct IndexFile {
    path: PathBuf,
    oid: String,
}

fn index_files(git: &Git<'_>, scope: &Path) -> anyhow::Result<Vec<IndexFile>> {
    let listing = git
        .command()
        .args(["ls-files", "--stage", "-z", "--"])
        .arg(scope)
        .output()?;
    anyhow::ensure!(
        listing.status.success(),
        "git ls-files: {}",
        String::from_utf8_lossy(&listing.stderr).trim()
    );
    nul_fields(&listing.stdout)
        .map(|record| {
            let tab = record
                .iter()
                .position(|&byte| byte == b'\t')
                .ok_or_else(|| anyhow::anyhow!("unexpected git index entry"))?;
            let path = PathBuf::from(git_path(&record[tab + 1..])?);
            let [mode, oid, stage] = std::str::from_utf8(&record[..tab])?
                .split(' ')
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|_| anyhow::anyhow!("unexpected git index entry"))?;
            anyhow::ensure!(stage == "0", "unmerged index entry {}", path.display());
            anyhow::ensure!(
                matches!(mode, "100644" | "100755"),
                "{} must be a regular file in the index",
                path.display()
            );
            anyhow::ensure!(
                path.starts_with(scope)
                    && path
                        .components()
                        .all(|part| matches!(part, Component::Normal(_))),
                "unexpected path in git index: {}",
                path.display()
            );
            Ok(IndexFile {
                path,
                oid: oid.to_owned(),
            })
        })
        .collect()
}

fn nul_fields(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes
        .split(|&byte| byte == 0)
        .filter(|part| !part.is_empty())
}

fn export_index(git: &Git<'_>, snapshot: &Path, scope: &Path) -> anyhow::Result<bool> {
    let files = index_files(git, scope)?;
    if files.is_empty() {
        return Ok(false);
    }

    // checkout-index runs checkout filters, which can even be selected by
    // unstaged .gitattributes. Read the index's blobs without conversion.
    let mut requests = tempfile::tempfile_in(snapshot)?;
    for file in &files {
        writeln!(requests, "{}", file.oid)?;
    }
    requests.rewind()?;
    let mut child = git
        .command()
        .arg("--no-replace-objects")
        .args(["cat-file", "--batch"])
        .stdin(Stdio::from(requests))
        .stdout(Stdio::piped())
        .spawn()?;
    let mut reader = BufReader::new(child.stdout.take().expect("stdout was piped"));
    let result = files.into_iter().try_for_each(|file| {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let size: u64 = header
            .trim_end()
            .strip_prefix(&format!("{} blob ", file.oid))
            .ok_or_else(|| {
                anyhow::anyhow!("could not read staged blob for {}", file.path.display())
            })?
            .parse()?;
        let dest = snapshot.join(&file.path);
        fs::create_dir_all(dest.parent().expect("export path has parent"))?;
        let written = io::copy(&mut reader.by_ref().take(size), &mut File::create(dest)?)?;
        let mut separator = [0];
        reader.read_exact(&mut separator)?;
        anyhow::ensure!(
            written == size && separator == *b"\n",
            "incomplete staged blob"
        );
        Ok(())
    });
    drop(reader);
    let status = child.wait()?;
    result?;
    anyhow::ensure!(status.success(), "git cat-file failed: {status}");
    Ok(true)
}

/// The installed script supplies the registry path even when its manifest
/// has been removed from the working tree. Confirm the manifest is in the
/// index before accepting that path in a pre-commit run.
pub(crate) fn registry_with_index_manifest() -> Option<PathBuf> {
    let value = env::var("MY_SKILLS_ROOT").ok()?;
    if value.is_empty() {
        return None;
    }
    let root = manifest::expand_path(&value);
    Git::discover(&root)
        .ok()?
        .run(&["ls-files", "--error-unmatch", "--", MANIFEST_FILE])
        .ok()?;
    Some(root)
}

pub(crate) fn pre_commit(git: &Git<'_>) -> anyhow::Result<Vec<(String, String)>> {
    let diff = git.run(&[
        "diff",
        "--cached",
        "--relative",
        "--name-only",
        "--diff-filter=ACMR",
        "-z",
        "--",
    ])?;
    if diff.is_empty() {
        return Ok(Vec::new());
    }
    let changed = nul_fields(&diff)
        .map(|bytes| git_path(bytes).map(PathBuf::from))
        .collect::<anyhow::Result<BTreeSet<_>>>()?;

    let snapshot = tempfile::tempdir()?;
    let manifest_path = Path::new(MANIFEST_FILE);
    let manifest = export_index(git, snapshot.path(), manifest_path).and_then(|exported| {
        if exported {
            Ok(Some(manifest::load_manifest(
                snapshot.path(),
                &Default::default(),
            )?))
        } else {
            Ok(None)
        }
    });
    let mut failures = Vec::new();
    let manifest = match manifest {
        Ok(manifest) => manifest,
        Err(error) if changed.contains(manifest_path) => {
            failures.push((MANIFEST_FILE.to_owned(), error.to_string()));
            None
        }
        // Prior manifest problems are reported by doctor. Use built-in
        // defaults when the index cannot provide a usable layout.
        Err(_) => None,
    };
    let configured_root = manifest
        .as_ref()
        .map_or("skills", |manifest| manifest.skills_root.as_str());
    let skills_root = Path::new(configured_root);
    let skills_root = skills_root.strip_prefix(git.root).unwrap_or(skills_root);
    if skills_root
        .components()
        .any(|part| !matches!(part, Component::CurDir | Component::Normal(_)))
    {
        return Ok(failures);
    }
    let skills_root: PathBuf = skills_root
        .components()
        .filter(|part| *part != Component::CurDir)
        .collect();
    let dirs: BTreeSet<_> = changed
        .iter()
        .filter_map(|path| {
            let mut relative = path.strip_prefix(&skills_root).ok()?.components();
            let Component::Normal(skill) = relative.next()? else {
                return None;
            };
            relative.next()?;
            Some(skill)
        })
        .collect();

    for skill in dirs {
        let scope = skills_root.join(skill);
        let name = skill.to_string_lossy();
        let errors = match export_index(git, snapshot.path(), &scope) {
            Ok(true) => skill_errors(&snapshot.path().join(scope), &name, manifest.as_ref()),
            Ok(false) => continue,
            Err(error) => vec![error.to_string()],
        };
        failures.extend(errors.into_iter().map(|error| (name.to_string(), error)));
    }
    Ok(failures)
}

fn skill_errors(path: &Path, name: &str, manifest: Option<&Manifest>) -> Vec<String> {
    validate::gate_errors(path, name, manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_hook_path_normalizes_separators_on_windows() {
        #[cfg(windows)]
        {
            assert_eq!(
                display_hook_path(Path::new(
                    r"C:/Users/example/my-agent-skills/.git/hooks\pre-commit"
                )),
                r"C:\Users\example\my-agent-skills\.git\hooks\pre-commit"
            );
            assert_eq!(
                display_hook_path(Path::new(
                    r"\\?\C:\Users\example\my-agent-skills\.git\hooks\pre-commit"
                )),
                r"C:\Users\example\my-agent-skills\.git\hooks\pre-commit"
            );
            assert_eq!(
                display_hook_path(Path::new(
                    r"//server/share/my-agent-skills/.git/hooks\pre-commit"
                )),
                r"\\server\share\my-agent-skills\.git\hooks\pre-commit"
            );
        }
        #[cfg(not(windows))]
        {
            assert_eq!(
                display_hook_path(Path::new(
                    "/Users/example/my-agent-skills/.git/hooks/pre-commit"
                )),
                "/Users/example/my-agent-skills/.git/hooks/pre-commit"
            );
        }
    }

    #[test]
    fn inspect_hook_error_normalizes_path_separators() {
        #[cfg(windows)]
        {
            let err = inspect_hook_error(
                Path::new(r"C:/Users/example/my-agent-skills/.git/hooks\pre-commit"),
                io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
            );
            assert_eq!(
                err.to_string(),
                r"C:\Users\example\my-agent-skills\.git\hooks\pre-commit: access denied"
            );
        }
        #[cfg(not(windows))]
        {
            let err = inspect_hook_error(
                Path::new("/Users/example/my-agent-skills/.git/hooks/pre-commit"),
                io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
            );
            assert_eq!(
                err.to_string(),
                "/Users/example/my-agent-skills/.git/hooks/pre-commit: access denied"
            );
        }
    }
}
