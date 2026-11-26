#[allow(dead_code)]
mod support_g;

use std::fs::{self, File, Permissions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::prelude::*;
use predicates::{prelude::PredicateBooleanExt, str::contains};
use support_g::{Fixture, MANIFEST, skill};

const HOOKS: [&str; 5] = [
    "pre-commit",
    "post-commit",
    "post-merge",
    "post-rewrite",
    "post-checkout",
];
const MARKER: &str = "# managed-by: my-skills hooks v1";
const CUSTOM_SKIP: &str = "Git hooks: skipped (core.hooksPath or a linked hooks folder is set; run 'my-skills hooks install' to install there)";
const SUBDIR_SKIP: &str = "Git hooks: skipped (inside a subdirectory of a larger Git repository)";
const NON_GIT_SKIP: &str = "Git hooks: skipped (not a Git repository)";
const BOOTSTRAP_COMPLETE: &str =
    "Bootstrap complete. Run `my-skills doctor` to inspect registry configuration.";

fn hooks(registry: &Path) -> PathBuf {
    registry.join(".git/hooks")
}

fn assert_managed(dir: &Path, names: &[&str]) {
    for name in names {
        let script = fs::read_to_string(dir.join(name)).unwrap();
        assert_eq!(script.lines().nth(1), Some(MARKER), "{name}");
    }
}

fn assert_absent(dir: &Path, names: &[&str]) {
    for name in names {
        assert!(
            !dir.join(name).exists(),
            "unexpectedly found {name} in {dir:?}"
        );
    }
}

fn snapshot(path: &Path) -> (Vec<u8>, SystemTime, Permissions) {
    let metadata = path.metadata().unwrap();
    (
        fs::read(path).unwrap(),
        metadata.modified().unwrap(),
        metadata.permissions(),
    )
}

fn set_old_mtime(path: &Path) -> (Vec<u8>, SystemTime, Permissions) {
    let old = UNIX_EPOCH + std::time::Duration::from_secs(978_307_200);
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    snapshot(path)
}

fn snapshot_dir(dir: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime, Permissions)> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let (text, modified, permissions) = snapshot(&entry.path());
            (
                PathBuf::from(entry.file_name()),
                text,
                modified,
                permissions,
            )
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn init_registry(fixture: &Fixture, target: &Path) -> Command {
    let mut cmd = fixture.command();
    cmd.arg("init-registry").arg(target).arg("--no-defaults");
    cmd
}

fn install(fixture: &Fixture) {
    fixture
        .command()
        .args(["hooks", "install"])
        .assert()
        .success();
}

fn enable_hook_binary(fixture: &Fixture) {
    let binary = assert_cmd::cargo::cargo_bin("my-skills");
    fs::copy(&binary, fixture.bin.join(binary.file_name().unwrap())).unwrap();
}

fn committed_registry() -> Fixture {
    let fixture = Fixture::isolated();
    fixture.init();
    fixture.commit();
    fixture
}

fn clone_registry(fixture: &Fixture) -> PathBuf {
    fixture.git(&["clone", "-q", ".", "../clone"]);
    fixture.base.join("clone")
}

fn run_in(fixture: &Fixture, registry: &Path) -> Command {
    let mut cmd = fixture.command();
    cmd.current_dir(registry).env("MY_SKILLS_ROOT", registry);
    cmd
}

fn stage_invalid_skill(fixture: &Fixture, worktree: &Path, registry_prefix: &Path) {
    let skill = registry_prefix.join("skills/rejected/reference.txt");
    let file = worktree.join(&skill);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, "skill without a SKILL.md\n").unwrap();
    fixture
        .git_command(&["add", "--"])
        .current_dir(worktree)
        .arg(skill)
        .assert()
        .success();
}

fn assert_pre_commit_blocks_without_filling(
    fixture: &Fixture,
    worktree: &Path,
    registry_prefix: &Path,
) {
    stage_invalid_skill(fixture, worktree, registry_prefix);
    fixture
        .git_command(&["commit", "-qm", "invalid skill"])
        .current_dir(worktree)
        .assert()
        .code(1)
        .stderr(
            contains("[BLOCKED] rejected: SKILL.md not found")
                .and(contains("added missing hook").not()),
        );
}

fn set_copy_target(fixture: &Fixture) -> PathBuf {
    let copy = fixture.base.join("copy-host");
    let path = serde_json::to_string(&copy).unwrap();
    fixture.write(
        "my-skills.toml",
        &format!(
            "{MANIFEST}\n[targets.claude]\nenabled = false\n\n\
         [targets.codex]\nenabled = true\npath = {path}\ninstall_mode = \"copy\"\n\n\
         [targets.agy]\nenabled = false\n\n[skills.sample]\nenabled = true\n"
        ),
    );
    fixture.write("skills/sample/SKILL.md", &skill("sample"));
    copy
}

#[test]
fn init_registry_installs_all_five_hooks() {
    let fixture = Fixture::isolated();
    let target = fixture.base.join("new-registry");
    let output = init_registry(&fixture, &target).assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let committed = stdout.find("Created initial registry commit").unwrap();
    let installed = stdout.find("installed: ").unwrap();
    let created = stdout.find("Created private skill registry at ").unwrap();
    assert!(committed < installed && installed < created, "{stdout}");
    assert_managed(&hooks(&target), &HOOKS);
}

#[test]
fn init_registry_installs_hooks_even_when_the_initial_commit_fails() {
    let fixture = Fixture::isolated();
    let target = fixture.base.join("uncommitted");
    init_registry(&fixture, &target)
        .env("GIT_AUTHOR_NAME", "")
        .assert()
        .success()
        .stdout(contains(
            "Initial git commit skipped; configure git user to commit",
        ));
    assert_managed(&hooks(&target), &HOOKS);
}

#[test]
fn init_registry_respects_no_git_and_no_hooks() {
    let fixture = Fixture::isolated();
    let no_git = fixture.base.join("no-git");
    init_registry(&fixture, &no_git)
        .arg("--no-git")
        .assert()
        .success()
        .stdout(
            contains("Git hooks:")
                .not()
                .and(contains("installed:").not()),
        );
    assert!(!no_git.join(".git").exists());

    let no_hooks = fixture.base.join("no-hooks");
    init_registry(&fixture, &no_hooks)
        .arg("--no-hooks")
        .assert()
        .success()
        .stdout(contains("hooks:").not().and(contains("installed:").not()));
    assert!(no_hooks.join(".git").is_dir());
    assert_absent(&hooks(&no_hooks), &HOOKS);
}

#[test]
fn bootstrap_installs_hooks_after_sync_in_a_clone() {
    let fixture = committed_registry();
    let copy = set_copy_target(&fixture);
    fixture.commit();
    let clone = clone_registry(&fixture);
    assert_absent(&hooks(&clone), &HOOKS);
    let cache = fixture.base.join("xdg-config/my-skills/root");
    assert!(!cache.exists());
    let result = run_in(&fixture, &clone)
        .env_remove("MY_SKILLS_ROOT")
        .arg("bootstrap")
        .assert()
        .success()
        .stderr(contains("note:").not());
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    let connecting = stdout
        .find("Connecting enabled skills into agent hosts...")
        .unwrap();
    let installed = stdout.find("installed: ").unwrap();
    let complete = stdout.find(BOOTSTRAP_COMPLETE).unwrap();
    assert!(connecting < installed && installed < complete, "{stdout}");
    assert!(copy.join("sample/SKILL.md").is_file());
    assert_managed(&hooks(&clone), &HOOKS);
    assert_eq!(
        Path::new(fs::read_to_string(cache).unwrap().trim())
            .canonicalize()
            .unwrap(),
        clone
    );
}

#[test]
fn bootstrap_saves_the_registry_when_only_hooks_were_written() {
    let fixture = committed_registry();
    set_copy_target(&fixture);
    fixture.commit();
    let clone = clone_registry(&fixture);
    let cache = fixture.base.join("xdg-config/my-skills/root");
    // Connect everything first with an explicit root, which is never saved, so
    // bootstrap's sync has nothing left to write.
    run_in(&fixture, &clone)
        .env_remove("MY_SKILLS_ROOT")
        .arg("--root")
        .arg(&clone)
        .args(["sync", "--yes"])
        .assert()
        .success();
    assert!(!cache.exists());
    run_in(&fixture, &clone)
        .env_remove("MY_SKILLS_ROOT")
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains("installed: "))
        .stderr(contains("note:").not());
    assert_managed(&hooks(&clone), &HOOKS);
    assert_eq!(
        Path::new(fs::read_to_string(cache).unwrap().trim())
            .canonicalize()
            .unwrap(),
        clone
    );
}

#[test]
fn bootstrap_only_fills_missing_hooks() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    install(&fixture);
    let kept: Vec<_> = HOOKS[..3]
        .iter()
        .map(|name| {
            let path = dir.join(name);
            let before = set_old_mtime(&path);
            (path, before)
        })
        .collect();
    let missing: Vec<_> = HOOKS[3..]
        .iter()
        .map(|name| {
            let path = dir.join(name);
            let explicit_wrapper = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            (path, explicit_wrapper)
        })
        .collect();

    let result = fixture.command().arg("bootstrap").assert().success();
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("installed: "))
            .count(),
        2,
        "{stdout}"
    );
    assert_managed(&dir, &HOOKS);
    for (path, before) in kept {
        assert_eq!(snapshot(&path), before, "{path:?}");
    }
    for (path, explicit_wrapper) in missing {
        assert_eq!(fs::read(&path).unwrap(), explicit_wrapper, "{path:?}");
    }
}

#[test]
fn setup_dry_runs_preview_without_writing() {
    let fixture = committed_registry();
    let target = fixture.base.join("preview");
    let dir = hooks(&fixture.registry);
    for (option, expected) in [
        (None, "yes"),
        (Some("--no-git"), "no"),
        (Some("--no-hooks"), "no"),
    ] {
        let mut cmd = init_registry(&fixture, &target);
        cmd.arg("--dry-run")
            .args(option)
            .assert()
            .success()
            .stdout(contains(format!("  git hooks: {expected}\n")));
        assert!(!target.exists());
    }
    assert!(!fixture.base.join("xdg-config/my-skills/root").exists());

    let before = snapshot_dir(&dir);
    let result = fixture
        .command()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    let would_install: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("  would install: "))
        .collect();
    assert_eq!(would_install.len(), HOOKS.len(), "{stdout}");
    for (line, name) in would_install.iter().zip(HOOKS) {
        assert!(line.ends_with(name), "{line}");
    }
    assert_eq!(snapshot_dir(&dir), before);

    install(&fixture);
    let before = snapshot_dir(&dir);
    fixture
        .command()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("  git hooks: already installed\n"));
    assert_eq!(snapshot_dir(&dir), before);

    let foreign = dir.join("pre-commit");
    fs::write(&foreign, "#!/bin/sh\nexit 0\n").unwrap();
    set_old_mtime(&foreign);
    let foreign_only = snapshot_dir(&dir);
    fixture
        .command()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success()
        .stdout(
            contains("  would leave foreign hook: ")
                .and(contains("already installed").not())
                .and(contains("would install:").not()),
        );
    assert_eq!(snapshot_dir(&dir), foreign_only);

    fs::remove_file(dir.join("post-commit")).unwrap();
    let before = snapshot_dir(&dir);
    let result = fixture
        .command()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("already installed").not());
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    for (prefix, name) in [
        ("  would leave foreign hook: ", "pre-commit"),
        ("  would install: ", "post-commit"),
    ] {
        assert!(
            stdout
                .lines()
                .any(|line| line.starts_with(prefix) && line.ends_with(name)),
            "{stdout}"
        );
    }
    assert_eq!(snapshot_dir(&dir), before);
    fixture
        .command()
        .args(["bootstrap", "--dry-run", "--no-hooks"])
        .assert()
        .success()
        .stdout(
            contains("git hooks:")
                .not()
                .and(contains("would install:").not())
                .and(contains("would leave foreign hook:").not()),
        );
    assert_eq!(snapshot_dir(&dir), before);
}

#[test]
fn bootstrap_and_explicit_install_preserve_foreign_hooks() {
    for args in [&["bootstrap"][..], &["hooks", "install"]] {
        let fixture = committed_registry();
        let dir = hooks(&fixture.registry);
        let foreign = dir.join("pre-commit");
        fs::write(&foreign, "#!/bin/sh\nexit 0\n").unwrap();
        let before = set_old_mtime(&foreign);
        fixture.command().args(args).assert()
            .code(i32::from(args[0] == "hooks"))
            .stderr(contains("Foreign hook at ").and(contains("pre-commit. Add this line to that hook:\n  my-skills hooks run pre-commit \"$@\"\n")));
        assert_managed(&dir, &HOOKS[1..]);
        assert_eq!(snapshot(&foreign), before);
    }
}

#[test]
fn pull_with_changes_restores_missing_post_commit() {
    let fixture = committed_registry();
    let clone = clone_registry(&fixture);
    enable_hook_binary(&fixture);
    run_in(&fixture, &clone)
        .args(["hooks", "install"])
        .assert()
        .success();
    fs::remove_file(hooks(&clone).join("post-commit")).unwrap();
    fixture.write("note.txt", "upstream\n");
    fixture.commit();

    fixture
        .git_command(&["pull", "--ff-only"])
        .current_dir(&clone)
        .assert()
        .success()
        .stderr(contains("my-skills: added missing hook post-commit\n"));
    assert_eq!(
        fs::read_to_string(clone.join("note.txt")).unwrap(),
        "upstream\n"
    );
    assert_managed(&hooks(&clone), &["post-commit"]);
}

#[test]
fn commit_runs_the_post_commit_hook_that_pre_commit_just_restored() {
    let fixture = committed_registry();
    let copy = set_copy_target(&fixture);
    fixture.commit();
    fixture.command().arg("bootstrap").assert().success();
    enable_hook_binary(&fixture);
    let copied = copy.join("sample/SKILL.md");
    let initial = skill("sample");
    let changed = format!("{initial}\nCommitted edit.\n");
    assert_eq!(fs::read_to_string(&copied).unwrap(), initial);
    let dir = hooks(&fixture.registry);
    fs::remove_file(dir.join("post-commit")).unwrap();

    fixture.write("skills/sample/SKILL.md", &changed);
    fixture.git(&["add", "skills"]);
    fixture
        .git_command(&["commit", "-qm", "edit sample"])
        .assert()
        .success()
        .stderr(contains("my-skills: added missing hook post-commit\n"));
    assert_managed(&dir, &["post-commit"]);
    assert_eq!(fs::read_to_string(copied).unwrap(), changed);
}

#[test]
fn up_to_date_pull_leaves_the_missing_hook_for_the_next_commit() {
    let fixture = committed_registry();
    let clone = clone_registry(&fixture);
    enable_hook_binary(&fixture);
    run_in(&fixture, &clone)
        .args(["hooks", "install"])
        .assert()
        .success();
    let dir = hooks(&clone);
    fs::remove_file(dir.join("post-commit")).unwrap();

    fixture
        .git_command(&["pull", "--ff-only"])
        .current_dir(&clone)
        .assert()
        .success()
        .stdout(contains("Already up to date"))
        .stderr(contains("added missing hook").not());
    assert_absent(&dir, &["post-commit"]);
    fixture
        .git_command(&["commit", "--allow-empty", "-qm", "local"])
        .current_dir(&clone)
        .assert()
        .success()
        .stderr(contains("my-skills: added missing hook post-commit\n"));
    assert_managed(&dir, &["post-commit"]);
}

#[test]
fn blocked_pre_commit_still_restores_missing_hooks() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    enable_hook_binary(&fixture);
    install(&fixture);
    fs::remove_file(dir.join("post-commit")).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]).stdout;
    fixture.write("skills/sample/reference.txt", "skill without a SKILL.md\n");
    fixture.git(&["add", "skills"]);

    fixture
        .git_command(&["commit", "-qm", "invalid"])
        .assert()
        .code(1)
        .stderr(
            contains("my-skills: added missing hook post-commit\n")
                .and(contains("[BLOCKED] sample:")),
        );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).stdout, head);
    assert_managed(&dir, &["post-commit"]);
}

#[test]
fn bootstrap_and_git_hooks_leave_a_linked_worktrees_shared_hooks_alone() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    enable_hook_binary(&fixture);
    install(&fixture);
    fixture.git(&["worktree", "add", "-qb", "linked", "../linked"]);
    let linked = fixture.base.join("linked");
    fs::remove_file(dir.join("post-commit")).unwrap();
    let before = snapshot_dir(&dir);

    run_in(&fixture, &linked)
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains("Git hooks: skipped (inside a linked worktree)\n"));
    assert_pre_commit_blocks_without_filling(&fixture, &linked, Path::new(""));
    assert_eq!(snapshot_dir(&dir), before);
}

#[test]
fn bootstrap_reports_a_non_git_registry_unless_disabled() {
    let fixture = Fixture::isolated();
    fixture
        .command()
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains(format!("{NON_GIT_SKIP}\n{BOOTSTRAP_COMPLETE}\n")));
    fixture
        .command()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("  git hooks: skipped (not a Git repository)\n"));
    assert!(!fixture.registry.join(".git").exists());

    fixture.init();
    let dir = hooks(&fixture.registry);
    let before = snapshot_dir(&dir);
    fixture
        .command()
        .args(["bootstrap", "--no-hooks"])
        .assert()
        .success()
        .stdout(
            contains(BOOTSTRAP_COMPLETE)
                .and(contains("Git hooks:").not())
                .and(contains("installed:").not()),
        )
        .stderr(contains("hook").not());
    assert_eq!(snapshot_dir(&dir), before);
}

#[test]
fn only_explicit_install_writes_to_a_configured_hooks_path() {
    let fixture = committed_registry();
    enable_hook_binary(&fixture);
    let dir = fixture.base.join("outside");
    let git_path = my_skills::platform::paths::display_path(&dir).replace('\\', "/");
    fixture
        .git_command(&["config", "core.hooksPath"])
        .arg(git_path)
        .assert()
        .success();
    install(&fixture);
    for name in &HOOKS[1..] {
        fs::remove_file(dir.join(name)).unwrap();
    }
    set_old_mtime(&dir.join("pre-commit"));
    let external_before = snapshot_dir(&dir);
    let default = hooks(&fixture.registry);
    let default_before = snapshot_dir(&default);

    assert_pre_commit_blocks_without_filling(&fixture, &fixture.registry, Path::new(""));
    fixture.git(&["reset", "-q", "--", "skills"]);
    fixture
        .command()
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains(CUSTOM_SKIP));
    assert_eq!(snapshot_dir(&dir), external_before);
    assert_eq!(snapshot_dir(&default), default_before);

    install(&fixture);
    assert_managed(&dir, &HOOKS);
    assert_eq!(snapshot_dir(&default), default_before);
}

#[cfg(unix)]
#[test]
fn read_only_hooks_emit_one_warning_without_failing_commit_or_pull() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = committed_registry();
    let clone = clone_registry(&fixture);
    enable_hook_binary(&fixture);
    let dir = hooks(&clone);
    run_in(&fixture, &clone)
        .args(["hooks", "install"])
        .assert()
        .success();
    fs::remove_file(dir.join("post-commit")).unwrap();
    let original = dir.metadata().unwrap().permissions();
    fs::set_permissions(&dir, Permissions::from_mode(0o555)).unwrap();
    let commit = fixture
        .git_command(&["commit", "--allow-empty", "-qm", "local commit"])
        .current_dir(&clone)
        .output()
        .unwrap();
    // Rewind the empty local commit without running a checkout hook, then fast-forward.
    fixture
        .git_command(&["reset", "--soft", "HEAD^"])
        .current_dir(&clone)
        .assert()
        .success();
    fixture.write("note.txt", "upstream\n");
    fixture.commit();
    let pull = fixture
        .git_command(&["pull", "--ff-only"])
        .current_dir(&clone)
        .output()
        .unwrap();
    fs::set_permissions(&dir, original).unwrap();

    for output in [commit, pull] {
        output.assert().success().stderr(
            predicates::str::contains("warning: my-skills could not add missing hooks: ").count(1),
        );
    }
    assert_absent(&dir, &["post-commit"]);
}

#[test]
fn bootstrap_warns_and_keeps_sync_results_when_the_hooks_directory_is_a_file() {
    let fixture = committed_registry();
    let copy = set_copy_target(&fixture);
    let dir = hooks(&fixture.registry);
    fs::rename(&dir, fixture.base.join("original-hooks")).unwrap();
    fs::write(&dir, "do not replace\n").unwrap();

    fixture
        .command()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would install:").not())
        .stderr(
            contains("warning: could not install git hooks: ")
                .count(1)
                .and(contains("hooks path is not a directory")),
        );
    assert!(!copy.exists());

    fixture
        .command()
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains(BOOTSTRAP_COMPLETE))
        .stderr(
            contains("warning: could not install git hooks: ")
                .count(1)
                .and(contains("hooks path is not a directory")),
        );
    assert_eq!(fs::read_to_string(dir).unwrap(), "do not replace\n");
    assert!(copy.join("sample/SKILL.md").is_file());
}

#[test]
fn an_obstructed_hooks_directory_warns_once_without_changing_pre_commit_validation() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    install(&fixture);
    stage_invalid_skill(&fixture, &fixture.registry, Path::new(""));
    let hook = || {
        fixture
            .command()
            .env("MY_SKILLS_ROOT", ".")
            .args(["hooks", "run", "pre-commit"])
            .output()
            .unwrap()
    };
    let unobstructed = hook();
    let expected_code = unobstructed.status.code().unwrap();
    unobstructed
        .assert()
        .code(1)
        .stderr("[BLOCKED] rejected: SKILL.md not found\n");

    fs::rename(&dir, fixture.base.join("original-hooks")).unwrap();
    fs::write(&dir, "do not replace\n").unwrap();
    hook().assert().code(expected_code).stderr(
        contains("warning: my-skills could not add missing hooks: ")
            .count(1)
            .and(contains("hooks path is not a directory"))
            .and(contains("[BLOCKED] rejected: SKILL.md not found\n")),
    );
    assert_eq!(fs::read_to_string(dir).unwrap(), "do not replace\n");
}

#[test]
fn hooks_only_bootstrap_obeys_the_write_policy_without_connecting_skills() {
    let fixture = committed_registry();
    let copy = set_copy_target(&fixture);
    let dir = hooks(&fixture.registry);
    let active = fixture.base.join("outside");
    fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
    fixture.cache(&active);
    let before = snapshot_dir(&dir);

    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["bootstrap", "--skip-skill-install"])
        .assert()
        .code(2)
        .stderr(contains("active registry"));
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["bootstrap", "--skip-skill-install", "--no-hooks"])
        .assert()
        .success()
        .stdout(contains("Git hooks:").not());
    assert_eq!(snapshot_dir(&dir), before);
    assert!(!copy.exists());

    let cache = fixture.base.join("xdg-config/my-skills/root");
    fs::remove_file(&cache).unwrap();
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["bootstrap", "--skip-skill-install"])
        .assert()
        .success()
        .stdout(contains("Connecting enabled skills").not());
    assert_managed(&dir, &HOOKS);
    assert!(!copy.exists());
    assert_eq!(
        Path::new(fs::read_to_string(cache).unwrap().trim())
            .canonicalize()
            .unwrap(),
        fixture.registry
    );
}

#[test]
fn a_complete_hook_set_stays_quiet_and_untouched() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    install(&fixture);
    let before: Vec<_> = HOOKS
        .iter()
        .map(|name| set_old_mtime(&dir.join(name)))
        .collect();
    fixture
        .command()
        .args(["hooks", "run", "post-checkout", "HEAD", "HEAD", "0"])
        .assert()
        .success()
        .stderr("");
    for (name, old) in HOOKS.into_iter().zip(before) {
        assert_eq!(snapshot(&dir.join(name)), old);
    }
    fixture
        .command()
        .args(["hooks", "status"])
        .assert()
        .success()
        .stdout(contains("  Missing hooks:").not());
    fs::remove_file(dir.join("post-commit")).unwrap();
    fixture
        .command()
        .args(["hooks", "status"])
        .assert()
        .success()
        .stdout(contains(
            "  Missing hooks: run 'my-skills hooks install' to add them.\n",
        ));
    fixture
        .command()
        .args(["hooks", "run", "post-checkout", "HEAD", "HEAD", "0"])
        .assert()
        .success()
        .stderr("my-skills: added missing hook post-commit\n");
    assert_managed(&dir, &["post-commit"]);
}

#[test]
fn a_complete_hook_set_only_runs_git_discovery_and_status() {
    let fixture = committed_registry();
    install(&fixture);
    let trace = fixture.base.join("git-trace.log");
    let trace_path = my_skills::platform::paths::display_path(&trace);
    // File checkout runs no Git commands of its own.
    fixture
        .command()
        .env("GIT_TRACE", trace_path)
        .args(["hooks", "run", "post-checkout", "HEAD", "HEAD", "0"])
        .assert()
        .success()
        .stderr("");
    let trace = fs::read_to_string(trace).unwrap();
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.contains("built-in: git "))
            .count(),
        2,
        "{trace}"
    );
    assert!(trace.contains("rev-parse --is-inside-work-tree"), "{trace}");
    assert!(
        trace.contains("rev-parse --path-format=absolute --git-path hooks"),
        "{trace}"
    );
}

#[test]
fn setup_leaves_parent_hooks_alone_when_the_registry_is_a_subdirectory() {
    let fixture = committed_registry();
    let nested = fixture.registry.join("nested");
    fixture.write("nested/my-skills.toml", MANIFEST);
    let dir = hooks(&fixture.registry);
    let before = snapshot_dir(&dir);

    run_in(&fixture, &nested)
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains(SUBDIR_SKIP));
    let target = fixture.registry.join("failed-init");
    init_registry(&fixture, &target)
        .env("GIT_DEFAULT_HASH", "invalid-hash")
        .assert()
        .success()
        .stdout(
            contains("git init failed; skipped initial commit\n").and(contains("Git hooks:").not()),
        );
    assert_eq!(snapshot_dir(&dir), before);
    assert!(!hooks(&target).exists());

    let outside = fixture.base.join("failed-init-outside");
    init_registry(&fixture, &outside)
        .env("GIT_DEFAULT_HASH", "invalid-hash")
        .assert()
        .success()
        .stdout(
            contains("git init failed; skipped initial commit\n").and(contains("Git hooks:").not()),
        );
    assert!(!hooks(&outside).exists());

    fixture.git(&["add", "nested/my-skills.toml"]);
    fixture.git(&["commit", "-qm", "nested registry"]);
    enable_hook_binary(&fixture);
    run_in(&fixture, &nested)
        .args(["hooks", "install"])
        .assert()
        .success();
    fs::remove_file(dir.join("post-commit")).unwrap();
    let before = snapshot_dir(&dir);
    assert_pre_commit_blocks_without_filling(&fixture, &fixture.registry, Path::new("nested"));
    assert_eq!(snapshot_dir(&dir), before);
}

#[test]
fn unknown_hook_does_not_fill_anything() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    let before = snapshot_dir(&dir);
    fixture
        .command()
        .args(["hooks", "run", "not-a-hook"])
        .assert()
        .code(2)
        .stderr(contains("unknown hook: not-a-hook"));
    assert_eq!(snapshot_dir(&dir), before);
}

#[cfg(any(unix, windows))]
#[test]
fn bootstrap_skips_linked_hooks_directories() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    let outside = fixture.base.join("outside");
    fs::write(outside.join("keep.txt"), "keep\n").unwrap();
    let before = snapshot_dir(&outside);
    fs::rename(&dir, fixture.base.join("original-hooks")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &dir).unwrap();
    #[cfg(windows)]
    junction::create(&outside, &dir).unwrap();

    fixture
        .command()
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains(CUSTOM_SKIP));
    assert_eq!(snapshot_dir(&outside), before);

    enable_hook_binary(&fixture);
    install(&fixture);
    fs::remove_file(outside.join("post-commit")).unwrap();
    let before = snapshot_dir(&outside);
    assert_pre_commit_blocks_without_filling(&fixture, &fixture.registry, Path::new(""));
    assert_eq!(snapshot_dir(&outside), before);
}

#[test]
fn bootstrap_returns_a_sync_failure_before_filling_hooks() {
    let fixture = committed_registry();
    let dir = hooks(&fixture.registry);
    let before = snapshot_dir(&dir);
    fixture.write("my-skills.toml", "invalid manifest\n");
    let sync = fixture.command().args(["sync", "--yes"]).output().unwrap();
    assert!(!sync.status.success());
    fixture
        .command()
        .arg("bootstrap")
        .assert()
        .code(sync.status.code().unwrap())
        .stdout(
            contains("Git hooks:")
                .not()
                .and(contains("installed:").not())
                .and(contains(BOOTSTRAP_COMPLETE).not()),
        );
    assert_eq!(snapshot_dir(&dir), before);
}
