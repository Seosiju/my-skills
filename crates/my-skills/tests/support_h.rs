//! Isolated filesystem and child-process environment for Stream H.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

pub(crate) struct Fixture {
    _temp: TempDir,
    pub(crate) root: PathBuf,
    pub(crate) registry: PathBuf,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let result = Self {
            _temp: temp,
            registry: root.join("registry"),
            root,
        };
        for dir in [
            "home",
            "state",
            "config",
            "data",
            "bin",
            "elsewhere",
            "registry",
        ] {
            fs::create_dir(result.root.join(dir)).unwrap();
        }
        result
    }

    pub(crate) fn repo() -> Self {
        let fixture = Self::new();
        fs::create_dir(fixture.registry.join("skills")).unwrap();
        fixture.write_manifest("");
        fixture
    }

    pub(crate) fn host(&self, host: &str) -> PathBuf {
        self.root.join("hosts").join(host)
    }

    pub(crate) fn manifest(&self) -> PathBuf {
        self.registry.join("my-skills.toml")
    }

    pub(crate) fn write_manifest(&self, extra: &str) {
        let mut text = "schema_version = 1\nskills_root = \"skills\"\n".to_owned();
        for (host, enabled) in [
            ("claude", true),
            ("codex", true),
            ("hermes", false),
            ("agy", false),
        ] {
            let path = serde_json::to_string(&self.host(host)).unwrap();
            text.push_str(&format!(
                "\n[targets.{host}]\nenabled = {enabled}\npath = {path}\n"
            ));
        }
        text.push_str(extra);
        fs::write(self.manifest(), text).unwrap();
    }

    pub(crate) fn config(&self) -> toml::Table {
        toml::from_str(&fs::read_to_string(self.manifest()).unwrap()).unwrap()
    }

    pub(crate) fn canonical(&self, skill: &str) -> PathBuf {
        self.registry.join("skills").join(skill)
    }

    pub(crate) fn state(&self) -> PathBuf {
        self.root.join("state/my-skills/state.json")
    }

    pub(crate) fn command(&self) -> Command {
        let bin = assert_cmd::cargo::cargo_bin!("my-skills");
        Command::from_std(self.process(bin))
    }

    pub(crate) fn process(&self, program: impl AsRef<std::ffi::OsStr>) -> Process {
        let mut cmd = Process::new(program);
        cmd.current_dir(&self.registry)
            .env("MY_SKILLS_ROOT", &self.registry)
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("LOCALAPPDATA", self.root.join("data"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .env("GIT_AUTHOR_NAME", "Test Author")
            .env("GIT_AUTHOR_EMAIL", "author@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test Author")
            .env("GIT_COMMITTER_EMAIL", "author@example.invalid");
        for var in [
            "GIT_CONFIG",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ] {
            cmd.env_remove(var);
        }
        cmd
    }
}

pub(crate) fn skill(parent: &Path, name: &str, description: &str, body: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}"),
    )
    .unwrap();
    dir
}

pub(crate) fn external(fixture: &Fixture, body: &str) -> PathBuf {
    skill(
        &fixture.root.join("external"),
        "brand",
        "A focused test skill.",
        body,
    )
}

pub(crate) fn json(command: &mut Command) -> Value {
    let assert = command.assert().success().stderr("");
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

pub(crate) fn native(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}

pub(crate) fn plain(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).into()
}

pub(crate) fn link_dir(source: &Path, dest: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, dest).unwrap();
    #[cfg(windows)]
    junction::create(source, dest).unwrap();
}
