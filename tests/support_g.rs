use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

pub const MANIFEST: &str = "schema_version = 1\nskills_root = \"skills\"\n";

pub struct Fixture {
    _temp: TempDir,
    pub base: PathBuf,
    pub registry: PathBuf,
    pub bin: PathBuf,
}

impl Fixture {
    pub fn isolated() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let registry = base.join("registry");
        let bin = base.join("bin");
        for dir in [&registry, &bin, &base.join("home"), &base.join("outside")] {
            fs::create_dir(dir).unwrap();
        }
        fs::write(registry.join("my-skills.toml"), MANIFEST).unwrap();
        Self {
            _temp: temp,
            base,
            registry,
            bin,
        }
    }

    pub fn command(&self) -> Command {
        self.command_from(cargo_bin("my-skills"))
    }

    pub fn command_from(&self, binary: impl AsRef<OsStr>) -> Command {
        let mut command = self.isolate(Command::new(binary), &self.registry);
        command.env("MY_SKILLS_ROOT", &self.registry);
        command
    }

    pub fn git_command(&self, args: &[&str]) -> Command {
        let mut command = self.isolate(Command::new("git"), &self.registry);
        command.args(args);
        command
    }

    pub fn git(&self, args: &[&str]) -> Output {
        let output = self.git_command(args).output().unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    pub fn init(&self) {
        self.git(&["init", "-q", "--initial-branch=main"]);
    }

    pub fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", "fixture"]);
    }

    pub fn write(&self, path: &str, contents: &str) {
        let dest = self.registry.join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(dest, contents).unwrap();
    }

    pub fn cache(&self, registry: &Path) {
        let cache = self.base.join("xdg-config/my-skills/root");
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(cache, format!("{}\n", registry.display())).unwrap();
    }

    pub fn fake_executable(&self, name: &str) -> PathBuf {
        let path = self
            .bin
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    pub fn path(&self) -> OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let suffix = std::env::consts::EXE_SUFFIX;
        let dirs = std::env::split_paths(&inherited).filter(|dir| {
            dir.is_absolute()
                && !["my-skills", "agy", "claude", "codex", "hermes"]
                    .iter()
                    .any(|name| {
                        dir.join(name).is_file() || dir.join(format!("{name}{suffix}")).is_file()
                    })
        });
        std::env::join_paths(std::iter::once(self.bin.clone()).chain(dirs)).unwrap()
    }

    fn isolate(&self, mut command: Command, cwd: &Path) -> Command {
        command
            .current_dir(cwd)
            .env("HOME", self.base.join("home"))
            .env("USERPROFILE", self.base.join("home"))
            .env("XDG_STATE_HOME", self.base.join("xdg-state"))
            .env("XDG_DATA_HOME", self.base.join("xdg-data"))
            .env("XDG_CONFIG_HOME", self.base.join("xdg-config"))
            .env("LOCALAPPDATA", self.base.join("local-app-data"))
            .env("APPDATA", self.base.join("xdg-config"))
            .env("PATH", self.path())
            .env("SHELL", "/bin/sh")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.base.join("gitconfig"))
            .env("GIT_AUTHOR_NAME", "Test Author")
            .env("GIT_AUTHOR_EMAIL", "author@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test Author")
            .env("GIT_COMMITTER_EMAIL", "author@example.invalid")
            .env_remove("MY_SKILLS_ROOT")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_TEMPLATE_DIR")
            .env_remove("GIT_NAMESPACE")
            .env_remove("GIT_CONFIG")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_EXEC_PATH");
        command
    }
}

pub fn skill(name: &str) -> String {
    format!("---\nname: {name}\ndescription: A valid example skill.\n---\n\n# Example\n")
}
