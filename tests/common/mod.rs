//! Shared helpers for every integration test file.
//!
//! One isolated child-process environment, one temporary registry fixture,
//! and the small output, JSON, and link helpers the test files share. Every
//! command a test runs gets its own home, XDG, `%LOCALAPPDATA%`, and Git
//! configuration under a temporary directory, so no test can read or write
//! the real registry, host folders, state, or root cache.
#![allow(dead_code)] // Each test target uses a different subset.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;
use assert_cmd::assert::{Assert, OutputAssertExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use url::Url;

/// The smallest valid registry manifest.
pub const MANIFEST: &str = "schema_version = 1\nskills_root = \"skills\"\n";

/// Links are junctions on Windows and symlinks elsewhere.
pub const LINK_KIND: &str = if cfg!(windows) { "junction" } else { "symlink" };

/// Hosts that `Fixture::write_manifest` configures, in manifest order.
pub const DEFAULT_TARGETS: [(&str, bool); 4] = [
    ("claude", true),
    ("codex", true),
    ("custom", false),
    ("agy", false),
];

/// Directories every fixture creates under its temporary root. The state,
/// config, and data homes stay absent so tests can prove nothing wrote there.
const LAYOUT: [&str; 5] = ["home", "bin", "elsewhere", "outside", "registry"];

/// Point every per-user location at `root` and pin Git to a private,
/// deterministic configuration. `MY_SKILLS_ROOT` is removed; callers that
/// want an explicit registry set it afterwards.
pub fn isolate(command: &mut Process, root: &Path) {
    command
        .env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("LOCALAPPDATA", root.join("data"))
        .env("APPDATA", root.join("config"))
        .env("SHELL", "/bin/sh")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("gitconfig"))
        .env("GIT_AUTHOR_NAME", "Test Author")
        .env("GIT_AUTHOR_EMAIL", "author@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test Author")
        .env("GIT_COMMITTER_EMAIL", "author@example.invalid");
    for var in [
        "MY_SKILLS_ROOT",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_TEMPLATE_DIR",
        "GIT_NAMESPACE",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_EXEC_PATH",
    ] {
        command.env_remove(var);
    }
}

/// A temporary root holding a registry, host folders, and isolated
/// home/XDG/data directories.
pub struct Fixture {
    _temp: TempDir,
    pub root: PathBuf,
    pub registry: PathBuf,
    pub bin: PathBuf,
}

impl Fixture {
    /// Empty registry directory without a manifest.
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for dir in LAYOUT {
            fs::create_dir(root.join(dir)).unwrap();
        }
        Self {
            _temp: temp,
            registry: root.join("registry"),
            bin: root.join("bin"),
            root,
        }
    }

    /// Registry with `skills/` and a manifest that targets claude and codex.
    pub fn repo() -> Self {
        let fixture = Self::new();
        fs::create_dir(fixture.registry.join("skills")).unwrap();
        fixture.write_manifest("");
        fixture
    }

    /// Registry with only the minimal manifest (no targets, no `skills/`).
    pub fn isolated() -> Self {
        let fixture = Self::new();
        fs::write(fixture.manifest(), MANIFEST).unwrap();
        fixture
    }

    pub fn host(&self, host: &str) -> PathBuf {
        self.root.join("hosts").join(host)
    }

    pub fn manifest(&self) -> PathBuf {
        self.registry.join("my-skills.toml")
    }

    pub fn canonical(&self, skill: &str) -> PathBuf {
        self.registry.join("skills").join(skill)
    }

    pub fn state(&self) -> PathBuf {
        self.root.join("state/my-skills/state.json")
    }

    pub fn root_cache(&self) -> PathBuf {
        self.root.join("config/my-skills/root")
    }

    /// Manifest with `DEFAULT_TARGETS` pointing at `host(..)`, then `extra`.
    pub fn write_manifest(&self, extra: &str) {
        self.write_targets(&DEFAULT_TARGETS, extra);
    }

    /// Manifest with the given targets pointing at `host(..)`, then `extra`.
    pub fn write_targets(&self, targets: &[(&str, bool)], extra: &str) {
        let mut text = MANIFEST.to_owned();
        for (host, enabled) in targets {
            let path = serde_json::to_string(&self.host(host)).unwrap();
            text.push_str(&format!(
                "\n[targets.{host}]\nenabled = {enabled}\npath = {path}\n"
            ));
        }
        text.push_str(extra);
        fs::write(self.manifest(), text).unwrap();
    }

    pub fn config(&self) -> toml::Table {
        toml::from_str(&fs::read_to_string(self.manifest()).unwrap()).unwrap()
    }

    /// Write `contents` to `path` relative to the registry.
    pub fn write(&self, path: &str, contents: &str) {
        let dest = self.registry.join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(dest, contents).unwrap();
    }

    /// Record `registry` in the isolated active-root cache.
    pub fn cache(&self, registry: &Path) {
        let cache = self.root_cache();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(cache, format!("{}\n", registry.display())).unwrap();
    }

    /// The CLI under test, run in the registry with `MY_SKILLS_ROOT` set.
    pub fn command(&self) -> Command {
        Command::from_std(self.process(assert_cmd::cargo::cargo_bin!("my-skills")))
    }

    /// Any program, isolated, in the registry, with `MY_SKILLS_ROOT` set.
    pub fn process(&self, program: impl AsRef<OsStr>) -> Process {
        let mut command = self.bare(program);
        command.env("MY_SKILLS_ROOT", &self.registry);
        command
    }

    /// Git in the registry, isolated, without `MY_SKILLS_ROOT`.
    pub fn git_command(&self, args: &[&str]) -> Process {
        let mut command = self.bare("git");
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

    /// `bin` first, then the inherited absolute `PATH` entries that hold no
    /// real my-skills or host executable.
    pub fn path(&self) -> OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let suffix = std::env::consts::EXE_SUFFIX;
        let dirs = std::env::split_paths(&inherited).filter(|dir| {
            dir.is_absolute()
                && !["my-skills", "agy", "claude", "codex"].iter().any(|name| {
                    dir.join(name).is_file() || dir.join(format!("{name}{suffix}")).is_file()
                })
        });
        std::env::join_paths(std::iter::once(self.bin.clone()).chain(dirs)).unwrap()
    }

    fn bare(&self, program: impl AsRef<OsStr>) -> Process {
        let mut command = Process::new(program);
        command.current_dir(&self.registry);
        isolate(&mut command, &self.root);
        command.env("PATH", self.path());
        command
    }
}

/// `SKILL.md` text for a valid example skill.
pub fn skill_md(name: &str) -> String {
    format!("---\nname: {name}\ndescription: A valid example skill.\n---\n\n# Example\n")
}

/// Write `parent/name/SKILL.md` and return the skill directory.
pub fn skill(parent: &Path, name: &str, description: &str, body: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}"),
    )
    .unwrap();
    dir
}

/// A `brand` skill outside the registry, ready to import.
pub fn external(fixture: &Fixture, body: &str) -> PathBuf {
    skill(
        &fixture.root.join("external"),
        "brand",
        "A focused test skill.",
        body,
    )
}

/// Run, require success with empty stderr, and parse stdout as JSON.
pub fn json(command: &mut Command) -> Value {
    let assert = command.assert().success().stderr("");
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

/// Run, require `code`, and parse stdout as JSON.
pub fn json_code(command: &mut Command, code: i32) -> Value {
    let assert = command.assert().code(code);
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

pub fn stdout(result: &Assert) -> &str {
    std::str::from_utf8(&result.get_output().stdout).unwrap()
}

pub fn stdout_json(result: &Assert) -> Value {
    serde_json::from_slice(&result.get_output().stdout).unwrap()
}

/// Expected text with the platform's line endings.
pub fn native(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}

/// A path as users see it, without the Windows verbatim prefix.
pub fn plain(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).into()
}

/// A directory link: a junction on Windows, a symlink elsewhere.
pub fn link_dir(source: &Path, dest: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, dest).unwrap();
    #[cfg(windows)]
    junction::create(source, dest).unwrap();
}

pub fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Sorted entry names of `dir`, or nothing when it does not exist.
pub fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

/// Windows ACL helpers for tests that make one file impossible to delete.
#[cfg(windows)]
pub mod acl {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use assert_cmd::assert::OutputAssertExt;

    fn system32(program: &str) -> PathBuf {
        let root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
        Path::new(&root).join("System32").join(program)
    }

    /// The current user as an `icacls` SID account (`*S-1-...`).
    pub fn account() -> String {
        let output = Command::new(system32("whoami.exe"))
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let csv = String::from_utf8(output.stdout).unwrap();
        let (_, sid) = csv.trim().rsplit_once(',').unwrap();
        format!("*{}", sid.trim_matches('"'))
    }

    pub fn icacls(path: &Path, action: &str, rule: &str) {
        Command::new(system32("icacls.exe"))
            .arg(path)
            .args([action, rule])
            .output()
            .unwrap()
            .assert()
            .success();
    }

    /// Deny deleting `file` (and its folder's children) without blocking a
    /// rename of the folder itself. Read-only attributes alone allow deletion.
    pub fn deny_delete(file: &Path, account: &str) {
        icacls(file, "/deny", &format!("{account}:(DE)"));
        icacls(file.parent().unwrap(), "/deny", &format!("{account}:(DC)"));
    }

    /// Undo `deny_delete` wherever `file` or its folder still exists.
    pub fn allow_delete(file: &Path, account: &str) {
        if file.exists() {
            icacls(file, "/remove:d", account);
        }
        if let Some(parent) = file.parent().filter(|parent| parent.exists()) {
            icacls(parent, "/remove:d", account);
        }
    }
}

// On Linux, a concurrent fork can briefly retain an open write handle to
// a freshly copied or written executable and cause exec to fail with ETXTBSY.
#[track_caller]
pub fn assert_with_busy_retry(command: &mut Process) -> Assert {
    const MAX_ATTEMPTS: usize = 100;
    let mut attempts = 0;
    loop {
        attempts += 1;
        match command.output() {
            Ok(output) => {
                return output
                    .assert()
                    .append_context("command", format!("{command:?}"));
            }
            Err(err) if cfg!(target_os = "linux") && err.raw_os_error() == Some(26) => {
                assert!(
                    attempts < MAX_ATTEMPTS,
                    "{command:?} stayed busy (ETXTBSY) after {MAX_ATTEMPTS} attempts: {err}"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => panic!("cannot run {command:?}: {err}"),
        }
    }
}

/// Self-update fixture: a copied CLI binary plus a local release server.
pub mod update {
    use super::*;

    pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
    pub const NEXT: &str = "1.999.0";
    pub const BINARY: &str = if cfg!(windows) {
        "my-skills.exe"
    } else {
        "my-skills"
    };

    pub fn archive_name() -> &'static str {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "my-skills-aarch64-apple-darwin.tar.gz",
            ("macos", "x86_64") => "my-skills-x86_64-apple-darwin.tar.gz",
            ("linux", "x86_64") => "my-skills-x86_64-unknown-linux-gnu.tar.gz",
            ("windows", "x86_64") => "my-skills-x86_64-pc-windows-msvc.zip",
            platform => panic!("unsupported release fixture platform: {platform:?}"),
        }
    }

    pub struct UpdateFixture {
        _temp: TempDir,
        pub root: PathBuf,
        pub assets: PathBuf,
        pub exe: PathBuf,
        release_list: PathBuf,
    }

    impl UpdateFixture {
        pub fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            // The space checks that every path survives quoting.
            let root = temp.path().join("update fixture");
            let api = root.join("api/repos/Seosiju/my-skills");
            for name in [
                "assets", "bin", "home", "registry", "config", "state", "data",
            ] {
                fs::create_dir_all(root.join(name)).unwrap();
            }
            fs::create_dir_all(&api).unwrap();
            let exe = root.join("bin").join(BINARY);
            fs::copy(env!("CARGO_BIN_EXE_my-skills"), &exe).unwrap();
            Self {
                _temp: temp,
                assets: root.join("assets"),
                root,
                exe,
                release_list: api.join("releases"),
            }
        }

        pub fn release_list(&self, versions: &[(&str, bool, bool)]) {
            let releases: Vec<_> = versions
                .iter()
                .map(|(tag, prerelease, draft)| {
                    json!({"tag_name": tag, "prerelease": prerelease, "draft": draft})
                })
                .collect();
            fs::write(&self.release_list, serde_json::to_vec(&releases).unwrap()).unwrap();
        }

        pub fn next_release(&self) {
            self.release_list(&[(&format!("v{NEXT}"), false, false)]);
        }

        pub fn asset_url(&self) -> Url {
            Url::from_directory_path(&self.assets).unwrap()
        }

        #[track_caller]
        pub fn run(&self, args: &[&str]) -> Assert {
            let mut command = Process::new(&self.exe);
            command.current_dir(&self.root);
            isolate(&mut command, &self.root);
            command
                .env("MY_SKILLS_ROOT", self.root.join("registry"))
                .env(
                    "MY_SKILLS_RELEASE_API_BASE_URL",
                    Url::from_directory_path(self.root.join("api"))
                        .unwrap()
                        .as_str(),
                )
                .env("MY_SKILLS_RELEASE_BASE_URL", self.asset_url().as_str());
            assert_with_busy_retry(command.args(args))
        }

        pub fn package(&self, binary: &Path) {
            let archive = package(binary);
            fs::write(self.assets.join(archive_name()), &archive).unwrap();
            self.checksum(&format!("{:x}", Sha256::digest(&archive)));
        }

        pub fn checksum(&self, digest: &str) {
            fs::write(
                self.assets.join("SHA256SUMS"),
                format!("{digest}  {}\r\n", archive_name()),
            )
            .unwrap();
        }

        pub fn compile_next_binary(&self) -> PathBuf {
            let source = self.root.join("candidate.rs");
            let candidate = self
                .root
                .join(format!("candidate{}", std::env::consts::EXE_SUFFIX));
            fs::write(
                &source,
                format!(
                    r#"fn main() {{
                    assert_eq!(std::env::args().nth(1).as_deref(), Some("--version"));
                    println!("my-skills {NEXT}");
                }}"#
                ),
            )
            .unwrap();
            let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let output = Process::new(compiler)
                .current_dir(&self.root)
                .arg(&source)
                .arg("-o")
                .arg(&candidate)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            candidate
        }
    }

    #[cfg(not(windows))]
    fn package(binary: &Path) -> Vec<u8> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        tar.append_path_with_name(binary, BINARY).unwrap();
        tar.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(windows)]
    fn package(binary: &Path) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(BINARY, options).unwrap();
        std::io::copy(&mut fs::File::open(binary).unwrap(), &mut zip).unwrap();
        zip.finish().unwrap().into_inner()
    }
}
