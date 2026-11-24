//! Release lookup and self-update library. Stream I. Spec §6.9.

use std::cmp::Ordering;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use url::Url;

use crate::cli::Channel;
use crate::platform::paths;

pub(crate) const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const API_BASE: &str = "https://api.github.com";
const RELEASES: &str = "https://github.com/Seosiju/my-skills/releases";
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
const DOCTOR_TIMEOUT: Duration = Duration::from_secs(2);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(90);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_BINARY_SIZE: u64 = 128 * 1024 * 1024;
const RELEASE_PAGE_SIZE: usize = 100;

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

pub(crate) struct Release {
    pub tag: String,
    version: Version,
}

impl Release {
    pub(crate) fn compare_current(&self) -> Result<Ordering> {
        Ok(Version::parse(CURRENT_VERSION)?.cmp_precedence(&self.version))
    }
}

fn base_url(variable: &str, fallback: &str) -> String {
    env::var(variable)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
        .trim_end_matches('/')
        .to_owned()
}

fn read_source(source: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if source.starts_with("file://") {
        let path = Url::parse(source)?
            .to_file_path()
            .map_err(|()| anyhow!("invalid file URL"))?;
        ensure!(path.metadata()?.is_file(), "expected a regular file");
        File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    } else {
        let config = ureq::Agent::config_builder()
            .user_agent(format!("my-skills/{CURRENT_VERSION}"))
            .timeout_global(Some(timeout));
        #[cfg(windows)]
        let config = config.tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        );
        let response = ureq::Agent::new_with_config(config.build())
            .get(source)
            .call()?;
        response
            .into_body()
            .into_reader()
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
    }
    ensure!(bytes.len() as u64 <= limit, "response exceeds size limit");
    Ok(bytes)
}

pub(crate) fn latest_release(channel: Channel) -> Result<Release> {
    find_release(channel, LOOKUP_TIMEOUT)
}

fn find_release(channel: Channel, timeout: Duration) -> Result<Release> {
    let base = base_url("MY_SKILLS_RELEASE_API_BASE_URL", API_BASE);
    let endpoint = format!("{base}/repos/Seosiju/my-skills/releases");
    let is_file = endpoint.starts_with("file://");
    let deadline = Instant::now() + timeout;
    let mut latest: Option<Release> = None;
    for page in 1.. {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "release lookup timed out");
        let source = if is_file {
            endpoint.clone()
        } else {
            format!("{endpoint}?per_page={RELEASE_PAGE_SIZE}&page={page}")
        };
        let bytes = read_source(&source, 8 * 1024 * 1024, remaining)
            .context("could not fetch GitHub releases")?;
        let releases: Vec<ApiRelease> =
            serde_json::from_slice(&bytes).context("invalid GitHub release list")?;
        let last_page = is_file || releases.len() < RELEASE_PAGE_SIZE;
        for release in releases.into_iter().filter(|release| !release.draft) {
            let Some(version) = release.tag_name.strip_prefix('v') else {
                continue;
            };
            let Ok(version) = Version::parse(version) else {
                continue;
            };
            if channel == Channel::Stable
                && (version.major != 1 || release.prerelease || !version.pre.is_empty())
            {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|current| version.cmp_precedence(&current.version).is_gt())
            {
                latest = Some(Release {
                    tag: release.tag_name,
                    version,
                });
            }
        }
        if last_page {
            break;
        }
    }
    latest.ok_or_else(|| match channel {
        Channel::Stable => anyhow!("no stable v1 releases found"),
        Channel::Main => anyhow!("no releases found"),
    })
}

fn archive_name() -> Result<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok("my-skills-aarch64-apple-darwin.tar.gz")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Ok("my-skills-x86_64-apple-darwin.tar.gz")
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        Ok("my-skills-x86_64-unknown-linux-gnu.tar.gz")
    } else if cfg!(all(windows, target_arch = "x86_64", target_env = "msvc")) {
        Ok("my-skills-x86_64-pc-windows-msvc.zip")
    } else {
        bail!(
            "no release archive for {} {}",
            env::consts::OS,
            env::consts::ARCH
        )
    }
}

pub(crate) struct UpdateDownload {
    pub archive_url: String,
    pub checksums_url: String,
    pub executable: PathBuf,
    archive_name: &'static str,
    version: Version,
}

impl UpdateDownload {
    pub(crate) fn new(release: Release) -> Result<Self> {
        let archive_name = archive_name()?;
        let base = base_url(
            "MY_SKILLS_RELEASE_BASE_URL",
            &format!("{RELEASES}/download/{}", release.tag),
        );
        Ok(Self {
            archive_url: format!("{base}/{archive_name}"),
            checksums_url: format!("{base}/SHA256SUMS"),
            executable: env::current_exe()?.canonicalize()?,
            archive_name,
            version: release.version,
        })
    }

    pub(crate) fn install(&self) -> Result<String> {
        let checksums = read_source(&self.checksums_url, 64 * 1024, DOWNLOAD_TIMEOUT)
            .context("could not download SHA256SUMS")?;
        let expected = checksum_for(std::str::from_utf8(&checksums)?, self.archive_name)?;
        let archive = read_source(&self.archive_url, MAX_BINARY_SIZE, DOWNLOAD_TIMEOUT)
            .with_context(|| format!("could not download {}", self.archive_name))?;
        ensure!(
            format!("{:x}", Sha256::digest(&archive)).eq_ignore_ascii_case(expected),
            "SHA-256 mismatch for {}",
            self.archive_name
        );

        let parent = self
            .executable
            .parent()
            .context("executable has no parent")?;
        let work = tempfile::Builder::new()
            .prefix(".my-skills-update-")
            .tempdir_in(parent)
            .context("could not stage the update beside the executable")?;
        let staged = work.path().join(if cfg!(windows) {
            "my-skills.exe"
        } else {
            "my-skills"
        });
        let mut binary = File::create(&staged)?;
        extract_binary(&archive, &mut binary).context("could not extract the release binary")?;
        set_executable(&binary)?;
        binary.sync_all()?;
        drop(binary);

        self.replace(&staged, work)
    }

    fn replace(&self, staged: &Path, work: TempDir) -> Result<String> {
        // Lock a separate inode so the same lock protects both binary versions.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(with_suffix(&self.executable, ".update-lock"))?;
        fs4::FileExt::try_lock(&lock).context("could not lock the executable for update")?;

        // Another updater may have replaced the executable while this one downloaded.
        verify_version(&self.executable, work.path(), CURRENT_VERSION)
            .context("installed executable changed before replacement")?;
        let name = self
            .executable
            .file_name()
            .context("executable has no name")?;
        let backup = with_suffix(&work.path().join(name), ".old");
        backup_executable(&self.executable, &backup).context("could not back up the executable")?;

        let result = fs::rename(staged, &self.executable)
            .context("could not replace the executable")
            .and_then(|()| {
                verify_version(&self.executable, work.path(), &self.version.to_string())
            });
        if let Err(error) = result {
            if let Err(rollback) = fs::rename(&backup, &self.executable) {
                let _ = work.keep();
                bail!(
                    "{error:#}; could not restore the old executable ({rollback}); backup at {}",
                    paths::display_path(&backup)
                );
            }
            return Err(error);
        }

        // Windows can keep the running image locked until this process exits.
        // If so, TempDir leaves the renamed executable at the backup path.
        result
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

fn checksum_for<'a>(checksums: &'a str, archive_name: &str) -> Result<&'a str> {
    let mut checksum = None;
    for line in checksums.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(digest), Some(filename)) = (fields.next(), fields.next()) {
            if filename.strip_prefix('*').unwrap_or(filename) != archive_name {
                continue;
            }
            ensure!(
                checksum.is_none()
                    && digest.len() == 64
                    && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "missing or invalid checksum for {archive_name}"
            );
            checksum = Some(digest);
        }
    }
    checksum.with_context(|| format!("missing or invalid checksum for {archive_name}"))
}

fn copy_binary(reader: impl Read, output: &mut File) -> Result<()> {
    let size = io::copy(&mut reader.take(MAX_BINARY_SIZE + 1), output)?;
    ensure!(
        (1..=MAX_BINARY_SIZE).contains(&size),
        "release binary is empty or exceeds size limit"
    );
    Ok(())
}

#[cfg(not(windows))]
fn extract_binary(archive: &[u8], output: &mut File) -> Result<()> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut found = false;
    for entry in tar.entries()? {
        let entry = entry?;
        if entry.path()?.as_ref() != Path::new("my-skills") {
            continue;
        }
        ensure!(
            !found && entry.header().entry_type().is_file(),
            "invalid my-skills entry"
        );
        copy_binary(entry, output)?;
        found = true;
    }
    ensure!(found, "missing my-skills entry");
    Ok(())
}

#[cfg(windows)]
fn extract_binary(archive: &[u8], output: &mut File) -> Result<()> {
    let mut zip = zip::ZipArchive::new(io::Cursor::new(archive))?;
    let binary = zip.by_name("my-skills.exe")?;
    ensure!(
        binary.is_file() && !binary.is_symlink(),
        "invalid my-skills.exe entry"
    );
    copy_binary(binary, output)
}

#[cfg(unix)]
fn set_executable(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn backup_executable(current: &Path, backup: &Path) -> io::Result<()> {
    fs::hard_link(current, backup)
}

#[cfg(windows)]
fn backup_executable(current: &Path, backup: &Path) -> io::Result<()> {
    fs::rename(current, backup)
}

fn verify_version(executable: &Path, work_dir: &Path, version: &str) -> Result<String> {
    let mut stdout = tempfile::tempfile_in(work_dir)?;
    let mut child = Command::new(executable)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(Stdio::null())
        .spawn()
        .context("could not run my-skills --version")?;
    let deadline = Instant::now() + VERIFY_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            outcome => {
                let _ = child.kill();
                let _ = child.wait();
                outcome.context("could not wait for my-skills --version")?;
                bail!("my-skills --version timed out");
            }
        }
    };
    ensure!(status.success(), "my-skills --version failed");
    stdout.rewind()?;
    let mut output = String::new();
    stdout.take(1024).read_to_string(&mut output)?;
    let observed = output.trim();
    ensure!(
        observed == format!("my-skills {version}"),
        "updated command did not report expected version {version}: {observed}"
    );
    Ok(observed.to_owned())
}

/// One line for `doctor`, without its `Update:  ` label.
pub fn update_status_line(no_check: bool) -> String {
    if no_check {
        return "not checked (--no-update-check)".into();
    }
    match find_release(Channel::Stable, DOCTOR_TIMEOUT)
        .and_then(|release| Ok((release.compare_current()?, release.tag)))
    {
        Ok((Ordering::Less, tag)) => format!("available {tag} (run 'my-skills update')"),
        Ok((Ordering::Equal, tag)) => format!("up to date (stable {tag})"),
        Ok((Ordering::Greater, tag)) => format!("ahead of latest stable {tag}"),
        Err(error) => format!("unknown ({error})"),
    }
}
