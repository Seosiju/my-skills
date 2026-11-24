//! Build-time seed files and the v0.4.1 registry scaffold. Stream H.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use include_dir::File;
use tempfile::NamedTempFile;

use crate::drive::link::{EntryKind, entry_kind};
use crate::registry::catalog::display_path;
use crate::registry::edit::native_text;
use crate::registry::manifest::MANIFEST_FILE;
use crate::{
    Error, Result, drive::hosts, platform::paths, registry::manifest, registry::root,
    registry::share,
};

const SEED_SKILLS: [(&str, bool); 3] = [
    ("cli-inventory", true),
    ("my-skills", true),
    ("my-jira", false),
];
const SEED_HOSTS: &str = r#"["claude", "codex", "agy"]"#;
// Record the package version that supplied the embedded seeds.
const SEED_REVISION: &str = env!("CARGO_PKG_VERSION");

macro_rules! seed_file {
    ($path:literal) => {
        File::new(
            $path,
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/skills/", $path)),
        )
    };
}

// Match the Python 0.4.x wheel's force-include list. Embedding the entire skills/ tree
// would put untracked machine-local files into the binary.
const SEED_FILES: [File<'static>; 10] = [
    seed_file!("cli-inventory/SKILL.md"),
    seed_file!("cli-inventory/references/inventory-schema.md"),
    seed_file!("cli-inventory/scripts/scan_tools.py"),
    seed_file!("my-skills/SKILL.md"),
    seed_file!("my-skills/references/hosts.md"),
    seed_file!("my-skills/references/json-and-dry-run.md"),
    seed_file!("my-skills/references/setup.md"),
    seed_file!("my-skills/references/share-and-audit.md"),
    seed_file!("my-jira/SKILL.md"),
    seed_file!("my-jira/config.example.json"),
];

const DEFAULT_REGISTRY: &str = "~/my-agent-skills";
const README_TITLE: &str = "Private Agent Skill Registry";
const INITIAL_COMMIT_PATHS: [&str; 3] = [".gitignore", "README.md", MANIFEST_FILE];

const MANIFEST: &str = r#"schema_version = 1
skills_root = "skills"

[defaults]
install_mode = "link"
collision = "error"
verify_after_install = true

[targets.claude]
enabled = true
scope = "user"
path = "~/.claude/skills"

[targets.codex]
enabled = true
scope = "user"
path = "~/.agents/skills"

[targets.agy]
enabled = false
scope = "user"
path = "~/.gemini/config/skills"
"#;

const GITIGNORE: &str =
    "my-skills.local.toml\nlocal/\n.omc/\n.omo/\n.omx/\n.DS_Store\n__pycache__/\n*.pyc\n";

const README_BODY: &str = r#"This repository is your canonical source for private Agent Skills.

Install the public Rust `my-skills` CLI using the release installer for your
platform. See the [tool repo README](https://github.com/Seosiju/my-skills/tree/main#install-the-cli).
Run `my-skills --version` to check the installation.

Return to this registry. Edit canonical skills under `skills/<name>/SKILL.md`
(run `my-skills resolve <path>` to find the canonical file from a host copy or
path), then preview (`--dry-run` or `--dry-run --json` for agents) and apply.
Add `--yes` after reviewing a plan for multiple hosts.

```bash
my-skills skills
my-skills install --dry-run
my-skills install
my-skills sync <skill> --host <host> --dry-run
my-skills sync <skill> --host <host>
```

After adding or changing skills, start a new host session if needed (Claude Code
picks up skills on restart; Codex detects changes automatically or on restart;
Antigravity CLI needs a new session and `/skills`).

Keep secrets, account data, personal memory, and machine-specific overrides out
of git. Store local data with:

```bash
my-skills data-path <skill> --create
```

Use `my-skills.local.toml` for machine-specific manifest overrides.
Antigravity CLI (`agy`) is opt-in: set `[targets.agy] enabled = true`
and include agy in each desired skill's hosts list before using --host all.
"#;

pub(crate) fn registry_target(path: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = path.filter(|path| !path.is_empty()) {
        return Ok(expand_user(path));
    }
    if !io::stdin().is_terminal() {
        return Ok(expand_user(DEFAULT_REGISTRY));
    }
    choose_target(
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
        &expand_user(DEFAULT_REGISTRY),
    )
}

fn choose_target(
    input: &mut impl io::BufRead,
    output: &mut impl Write,
    default: &Path,
) -> Result<PathBuf> {
    loop {
        write!(output, "Registry location [{DEFAULT_REGISTRY}]: ")
            .and_then(|()| output.flush())
            .map_err(|err| Error::io("stdout", err))?;
        let mut line = String::new();
        let count = input
            .read_line(&mut line)
            .map_err(|err| Error::io("stdin", err))?;
        if count == 0 {
            return Err(Error::Manifest("no registry location was entered".into()));
        }
        let path = line.trim();
        let target = if path.is_empty() {
            default.to_owned()
        } else {
            expand_user(path)
        };
        if !target.join(MANIFEST_FILE).exists() {
            return Ok(target);
        }
        writeln!(
            output,
            "{} already contains a my-skills registry. Enter a different path or use the existing registry.",
            display_path(&target)
        )
        .map_err(|err| Error::io("stdout", err))?;
    }
}

/// Like pathlib, preserve relative spelling and do not interpolate variables.
/// Collecting components also removes a trailing slash before checking a link.
pub(crate) fn expand_user(text: &str) -> PathBuf {
    let normalized: PathBuf = Path::new(text).components().collect();
    if let Ok(rest) = normalized.strip_prefix("~") {
        paths::home_dir().join(rest)
    } else if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

pub(crate) struct Scaffold {
    readme: String,
    previous_readme: Option<String>,
}

impl Scaffold {
    pub(crate) fn check(target: &Path) -> Result<Self> {
        let mut overlaps = Vec::new();
        for name in [MANIFEST_FILE, "skills", ".gitignore"] {
            if entry_kind(&target.join(name))? != EntryKind::Missing {
                overlaps.push(name);
            }
        }
        let readme_path = target.join("README.md");
        let mut title = README_TITLE.to_owned();
        let previous_readme = if entry_kind(&readme_path)? == EntryKind::Missing {
            None
        } else {
            let existing = if entry_kind(&readme_path)? == EntryKind::File {
                fs::read_to_string(&readme_path).ok()
            } else {
                None
            };
            match existing.as_deref().and_then(starter_title) {
                Some(starter) => title = starter.to_owned(),
                None => overlaps.push("README.md"),
            }
            existing
        };
        if !overlaps.is_empty() {
            // The command treats scaffold overlaps as a blocked write.
            return Err(Error::Refused(format!(
                "registry scaffold already exists: {}",
                overlaps.join(", ")
            )));
        }
        let subtitle = if title == README_TITLE {
            String::new()
        } else {
            format!("{README_TITLE}.\n\n")
        };
        Ok(Self {
            readme: format!("# {title}\n\n{subtitle}{README_BODY}"),
            previous_readme,
        })
    }

    pub(crate) fn create(&self, target: &Path, with_defaults: bool) -> Result<()> {
        let host_paths = || {
            hosts::ALL
                .iter()
                .map(|host| manifest::expand_path(host.default_user_path))
        };
        let skills = target.join("skills");
        share::ensure_outside_hosts(target, host_paths())?;
        share::ensure_outside_hosts(&skills, host_paths())?;
        share::ensure_outside_hosts(&root::root_cache_path(), host_paths())?;
        fs::create_dir_all(target).map_err(|err| Error::io(target, err))?;
        fs::create_dir(&skills).map_err(|err| Error::io(&skills, err))?;
        let mut created = Vec::new();
        let result = (|| {
            if with_defaults {
                write_seeds(&skills)?;
            }
            let manifest = scaffold_manifest(with_defaults);
            for (name, text) in [
                (MANIFEST_FILE, manifest.as_str()),
                (".gitignore", GITIGNORE),
            ] {
                let path = target.join(name);
                write_new(&path, text)?;
                created.push(path);
            }
            let readme = target.join("README.md");
            match &self.previous_readme {
                Some(previous) => replace_starter(&readme, previous, &self.readme)?,
                None => write_new(&readme, &self.readme)?,
            }
            Ok(())
        })();
        if result.is_err() {
            for path in created {
                let _ = fs::remove_file(path);
            }
            // The crate's single recursive delete refuses anything but a
            // real directory (spec §4 I2).
            let _ = crate::drive::apply::remove_real_dir(&skills);
        }
        result
    }
}

fn starter_title(text: &str) -> Option<&str> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(heading) = lines.next() else {
        return Some(README_TITLE);
    };
    if lines.next().is_some() {
        return None;
    }
    let title = heading.strip_prefix("# ")?.trim();
    Some(if title.is_empty() {
        README_TITLE
    } else {
        title
    })
}

fn scaffold_manifest(with_defaults: bool) -> String {
    use std::fmt::Write as _;

    let mut text = MANIFEST.to_owned();
    if with_defaults {
        for (name, enabled) in SEED_SKILLS {
            write!(
                text,
                "\n[skills.{name}]\nenabled = {enabled}\nhosts = {SEED_HOSTS}\nsource_type = \"builtin-seed\"\nsource_revision = \"{SEED_REVISION}\"\n"
            )
            .unwrap();
        }
    }
    text
}

fn write_seeds(skills: &Path) -> Result<()> {
    for seed in &SEED_FILES {
        let path = skills.join(seed.path());
        let parent = path.parent().expect("seed files have a parent");
        fs::create_dir_all(parent).map_err(|err| Error::io(parent, err))?;
        fs::write(&path, seed.contents()).map_err(|err| Error::io(&path, err))?;
    }
    Ok(())
}

fn write_new(path: &Path, text: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| Error::io(path, err))?;
    if let Err(err) = file.write_all(native_text(text).as_bytes()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(Error::io(path, err));
    }
    Ok(())
}

fn replace_starter(path: &Path, previous: &str, text: &str) -> Result<()> {
    let parent = path.parent().expect("README has a parent");
    let mut temp = NamedTempFile::new_in(parent).map_err(|err| Error::io(path, err))?;
    temp.write_all(native_text(text).as_bytes())
        .map_err(|err| Error::io(path, err))?;
    let permissions = fs::metadata(path)
        .map_err(|err| Error::io(path, err))?
        .permissions();
    temp.as_file()
        .set_permissions(permissions)
        .map_err(|err| Error::io(path, err))?;
    if entry_kind(path)? != EntryKind::File
        || fs::read_to_string(path).map_err(|err| Error::io(path, err))? != previous
    {
        return Err(Error::Manifest(format!(
            "{} changed during initialization",
            display_path(path)
        )));
    }
    temp.persist(path)
        .map_err(|err| Error::io(path, err.error))?;
    Ok(())
}

pub(crate) fn init_git(target: &Path, with_defaults: bool) {
    // Test for an executable before handling an existing .git directory.
    if let Err(err) = git(target, &["--version"]) {
        if err.kind() == io::ErrorKind::NotFound {
            println!("git command not found; skipped git init");
            return;
        }
    }
    if target.join(".git").exists() {
        println!("git repository already exists; skipped git init");
    } else if !matches!(git(target, &["init"]), Ok(true)) {
        println!("git init failed; skipped initial commit");
        return;
    }

    let mut paths = INITIAL_COMMIT_PATHS.to_vec();
    if with_defaults {
        paths.push("skills");
    }
    let mut add = vec!["add", "--"];
    add.extend(&paths);
    let _ = git(target, &add);
    let commit = ["commit", "-m", "chore: initialize my-skills registry"];
    if matches!(git(target, &commit), Ok(true)) {
        println!("Created initial registry commit");
    } else {
        println!("Initial git commit skipped; configure git user to commit");
    }
}

fn git(target: &Path, args: &[&str]) -> io::Result<bool> {
    Command::new("git")
        .args(args)
        .current_dir(target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_registry_prompt_accepts_empty_default() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("my-agent-skills");
        let mut messages = Vec::new();
        let target = choose_target(&mut &b"\n"[..], &mut messages, &default).unwrap();
        assert_eq!(target, default);
        assert_eq!(messages, b"Registry location [~/my-agent-skills]: ");
    }

    #[test]
    fn init_registry_prompt_reasks_for_existing_registry() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(MANIFEST_FILE), "schema_version = 1").unwrap();
        let alternate = temp.path().join("alternate");
        let answers = format!("{}\n{}\n", temp.path().display(), alternate.display());
        let mut messages = Vec::new();
        let default = temp.path().join("my-agent-skills");
        let target = choose_target(&mut answers.as_bytes(), &mut messages, &default).unwrap();
        assert_eq!(target, alternate);
        assert!(
            String::from_utf8(messages)
                .unwrap()
                .contains("already contains a my-skills registry")
        );
    }
}
