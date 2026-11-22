#![allow(dead_code)] // Imported by update_cli.rs. Cargo also builds this file as a test target.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use url::Url;

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

pub(crate) struct Fixture {
    _temp: TempDir,
    pub root: PathBuf,
    pub assets: PathBuf,
    pub exe: PathBuf,
    release_list: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("update fixture");
        let api = root.join("api/repos/Seosiju/my-skills");
        for name in [
            "assets",
            "bin",
            "home",
            "registry",
            "xdg-config",
            "xdg-state",
            "xdg-data",
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

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.exe);
        command
            .current_dir(&self.root)
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("APPDATA", self.root.join("xdg-config"))
            .env("LOCALAPPDATA", self.root.join("xdg-data"))
            .env("XDG_CONFIG_HOME", self.root.join("xdg-config"))
            .env("XDG_STATE_HOME", self.root.join("xdg-state"))
            .env("XDG_DATA_HOME", self.root.join("xdg-data"))
            .env("MY_SKILLS_ROOT", self.root.join("registry"))
            .env(
                "MY_SKILLS_RELEASE_API_BASE_URL",
                Url::from_directory_path(self.root.join("api"))
                    .unwrap()
                    .as_str(),
            )
            .env("MY_SKILLS_RELEASE_BASE_URL", self.asset_url().as_str());
        command
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
        let output = Command::new(compiler)
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
