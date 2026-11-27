//! Managed Git hooks: install, pre-commit validation, and post-commit refresh.

mod common;

/// Every managed Git hook, in install order.
const HOOKS: [&str; 5] = [
    "pre-commit",
    "post-commit",
    "post-merge",
    "post-rewrite",
    "post-checkout",
];

mod hooks_git {
    use std::fs;

    use crate::common::assert_with_busy_retry;
    use crate::common::{Fixture, MANIFEST, skill_md as skill};
    use assert_cmd::prelude::*;
    use predicates::prelude::PredicateBooleanExt;
    use predicates::str::contains;

    use super::HOOKS;

    #[test]
    fn install_dry_run_status_reinstall_and_uninstall_honor_hooks_path() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.git(&["config", "core.hooksPath", "custom hooks"]);
        let hooks = fixture.registry.join("custom hooks");

        fixture
            .command()
            .args(["hooks", "status"])
            .assert()
            .success()
            .stdout(contains("pre-commit      missing"))
            .stdout(contains("my-skills on PATH: no"));
        fixture
            .command()
            .args(["hooks", "install", "--dry-run"])
            .assert()
            .success()
            .stdout(contains("would install:"));
        assert!(!hooks.exists());

        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        for hook in HOOKS {
            let path = hooks.join(hook);
            let contents = fs::read_to_string(&path).unwrap();
            assert!(contents.starts_with("#!/bin/sh\n# managed-by: my-skills hooks v1\n"));
            assert!(contents.contains("command -v my-skills"));
            assert!(contents.contains(&format!("exec \"$MY_SKILLS\" hooks run {hook} \"$@\"")));
            assert!(!contents.contains('\r'));
            assert!(!fixture.registry.join(".git/hooks").join(hook).exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o755
                );
            }
        }
        let managed = hooks.join("post-checkout");
        fs::write(
            &managed,
            "#!/bin/sh\n# managed-by: my-skills hooks v0\n# replace me\n",
        )
        .unwrap();
        fixture.fake_executable("my-skills");
        fixture
            .command()
            .args(["hooks", "status"])
            .assert()
            .success()
            .stdout(contains("pre-commit      installed  v1"))
            .stdout(contains("post-checkout   installed  v0"))
            .stdout(contains("my-skills on PATH: yes"));
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        assert!(!fs::read_to_string(managed).unwrap().contains("replace me"));

        let foreign = hooks.join("post-merge");
        let foreign_text = "#!/bin/sh\n# foreign hook\n# managed-by: my-skills hooks v1\n";
        fs::write(&foreign, foreign_text).unwrap();
        fixture
            .command()
            .args(["hooks", "uninstall"])
            .assert()
            .success();
        assert_eq!(fs::read_to_string(&foreign).unwrap(), foreign_text);
        for hook in HOOKS.into_iter().filter(|hook| *hook != "post-merge") {
            assert!(!hooks.join(hook).exists());
        }
    }

    #[test]
    fn install_leaves_a_foreign_hook_in_place_and_installs_the_other_hooks() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write(".git/hooks/post-rewrite", "#!/bin/sh\necho foreign\n");
        fixture.write(
            ".git/hooks/pre-commit",
            "#!/bin/sh\n# managed-by: my-skills hooks v0\n",
        );
        let preview = fixture
            .command()
            .args(["hooks", "install", "--dry-run"])
            .assert()
            .code(1)
            .stderr(
                contains("my-skills hooks run post-rewrite \"$@\"")
                    .and(contains("existing foreign hooks were left in place")),
            )
            .stdout(contains("would install:").count(4));
        assert!(!String::from_utf8_lossy(&preview.get_output().stdout).contains("post-rewrite"));
        assert!(!fixture.registry.join(".git/hooks/post-merge").exists());
        assert_eq!(
            fs::read_to_string(fixture.registry.join(".git/hooks/pre-commit")).unwrap(),
            "#!/bin/sh\n# managed-by: my-skills hooks v0\n"
        );
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .code(1)
            .stderr(
                contains("my-skills hooks run post-rewrite \"$@\"")
                    .and(contains("existing foreign hooks were left in place")),
            )
            .stdout(contains("installed:").count(4));
        fixture
            .command()
            .args(["hooks", "status"])
            .assert()
            .success()
            .stdout(contains("post-rewrite    foreign"));
        assert_eq!(
            fs::read_to_string(fixture.registry.join(".git/hooks/post-rewrite")).unwrap(),
            "#!/bin/sh\necho foreign\n"
        );
        for hook in HOOKS.into_iter().filter(|name| *name != "post-rewrite") {
            let script =
                fs::read_to_string(fixture.registry.join(".git/hooks").join(hook)).unwrap();
            assert!(script.starts_with("#!/bin/sh\n# managed-by: my-skills hooks v1\n"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn links_are_neither_managed_hooks_nor_exportable_index_files() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::isolated();
        fixture.init();
        let payload = fixture.root.join("external-file");
        fs::write(&payload, "#!/bin/sh\n# managed-by: my-skills hooks v1\n").unwrap();
        let hook = fixture.registry.join(".git/hooks/post-rewrite");
        symlink(&payload, &hook).unwrap();
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .code(1);
        fixture
            .command()
            .args(["hooks", "uninstall"])
            .assert()
            .success();
        assert_eq!(fs::read_link(hook).unwrap(), payload);

        fixture.write("skills/example/SKILL.md", &skill("example"));
        symlink(&payload, fixture.registry.join("skills/example/link.txt")).unwrap();
        fixture.git(&["add", "."]);
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .code(1)
            .stderr(contains(
                "[BLOCKED] example: skills/example/link.txt must be a regular file in the index",
            ));
    }

    #[test]
    fn hooks_require_a_git_worktree() {
        let fixture = Fixture::isolated();
        for command in ["install", "uninstall", "status"] {
            fixture
                .command()
                .args(["hooks", command])
                .assert()
                .code(1)
                .stderr(contains("is not a Git worktree"));
        }
    }

    #[test]
    fn real_git_commit_runs_pre_commit_with_the_shell_quoted_binary_fallback() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.commit();
        let binary = fixture
            .root
            .join(format!("cli's copy{}", std::env::consts::EXE_SUFFIX));
        fs::copy(assert_cmd::cargo::cargo_bin("my-skills"), &binary).unwrap();
        assert_with_busy_retry(fixture.process(&binary).args(["hooks", "install"])).success();

        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture.git(&["add", "skills"]);
        fixture.write("skills/example/SKILL.md", "unstaged invalid content\n");
        fixture.git(&["commit", "-qm", "valid index"]);
        assert_eq!(
            String::from_utf8(
                fixture
                    .git(&["show", "HEAD:skills/example/SKILL.md"])
                    .stdout
            )
            .unwrap(),
            skill("example")
        );

        fixture.write("skills/example/reference.txt", "staged reference\n");
        fs::remove_file(fixture.registry.join("skills/example/SKILL.md")).unwrap();
        fixture.git(&["add", "skills"]);
        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture
            .git_command(&["commit", "-qm", "invalid index"])
            .assert()
            .code(1)
            .stderr(contains("[BLOCKED] example: SKILL.md not found"));
    }

    #[test]
    fn partial_git_commit_uses_the_index_selected_by_git() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write("note.txt", "initial\n");
        fixture.commit();
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        fixture.write("skills/example/reference.txt", "staged reference\n");
        fixture.git(&["add", "skills"]);
        fixture.write("note.txt", "good partial commit\n");
        fixture.git(&[
            "commit",
            "--only",
            "-qm",
            "commit one path",
            "--",
            "note.txt",
        ]);
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .code(1)
            .stderr(contains("[BLOCKED] example: SKILL.md not found"));
    }

    #[test]
    fn pre_commit_accepts_a_tracked_manifest_missing_from_the_worktree() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture.git(&["add", "."]);
        fs::remove_file(fixture.registry.join("my-skills.toml")).unwrap();
        fixture.git(&["commit", "-qm", "valid manifest from the index"]);
    }

    #[test]
    fn raw_index_blobs_ignore_worktree_attributes_and_checkout_filters() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.git(&["config", "filter.demo.smudge", "sed 1d"]);
        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture.git(&["add", "."]);
        fixture.write(".gitattributes", "SKILL.md filter=demo\n");
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .success();

        fixture.git(&["add", ".gitattributes"]);
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .success();
    }

    #[test]
    fn pre_commit_accepts_staged_binary_assets_without_inspecting_their_signature() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write("skills/example/SKILL.md", &skill("example"));
        let path = "skills/example/pixel.png";
        let png = include_bytes!("fixtures/binary-assets/pixel.png");
        fs::write(fixture.registry.join(path), png).unwrap();
        fixture.git(&["add", "."]);
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .success();

        fs::write(fixture.registry.join(path), b"not PNG\0").unwrap();
        fixture.git(&["add", "--", path]);
        fs::write(fixture.registry.join(path), png).unwrap();
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .success();
    }

    #[test]
    fn pre_commit_validates_staged_manifest_layout_without_blocking_host_warnings() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write(
            "my-skills.toml",
            concat!(
                "schema_version = 1\nskills_root = \"./training material\"\n",
                "[skills.example]\nhosts = [\"codex\"]\n[targets.claude]\nenabled = false\n"
            ),
        );
        let long = format!(
            "---\nname: example\ndescription: {}\n---\n",
            "a".repeat(1100)
        );
        fixture.write("training material/example/SKILL.md", &long);
        fixture.write(
            "training material/second/notes.txt",
            "Only a supporting file.\n",
        );
        fixture.git(&["add", "."]);
        fixture.write(
            "my-skills.toml",
            "this is intentionally malformed and unstaged!\n",
        );
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .code(1)
            .stderr(contains("[BLOCKED] example:").not())
            .stderr(contains("[BLOCKED] second: SKILL.md not found"));

        fixture.git(&["add", "my-skills.toml"]);
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .code(1)
            .stderr(contains("[BLOCKED] my-skills.toml: invalid TOML"));
    }

    #[test]
    fn pre_commit_ignores_disabled_targets_and_only_deleted_or_unchanged_skills() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write(
        "my-skills.toml",
        &format!(
            "{MANIFEST}[skills.example]\nhosts = [\"codex\"]\n[targets.codex]\nenabled = false\n"
        ),
    );
        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture.write("skills/old/SKILL.md", "invalid content from prior commit\n");
        fixture.write(
            "skills/unchanged/SKILL.md",
            "invalid content from prior commit\n",
        );
        fixture.commit();
        fixture.git(&["rm", "skills/old/SKILL.md"]);
        let long = format!(
            "---\nname: example\ndescription: {}\n---\n",
            "a".repeat(1100)
        );
        fixture.write("skills/example/SKILL.md", &long);
        fixture.git(&["add", "skills/example"]);
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .success();
    }

    #[test]
    fn pre_commit_exports_other_tracked_files_of_the_changed_skill() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture.write("skills/example/evidence.txt", "reference\n");
        fixture.commit();
        fixture.write(
            "skills/example/SKILL.md",
            &(skill("example") + "[Support](evidence.txt)\n"),
        );
        fixture.git(&["add", "skills/example/SKILL.md"]);
        fs::remove_file(fixture.registry.join("skills/example/evidence.txt")).unwrap();
        fixture
            .command()
            .args(["hooks", "run", "pre-commit"])
            .assert()
            .success();
    }

    #[test]
    fn nested_registry_hooks_find_the_manifest_from_gits_worktree_root() {
        let mut fixture = Fixture::isolated();
        fixture.init();
        let top = fixture.registry.clone();
        fixture.registry = top.join("nested registry");
        fixture.write("my-skills.toml", MANIFEST);
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        fixture.write("skills/example/reference.txt", "staged reference\n");
        fixture.registry = top;
        fixture.git(&["add", "."]);
        fixture
            .git_command(&["commit", "-qm", "initial"])
            .assert()
            .code(1)
            .stderr(contains("[BLOCKED] example: SKILL.md not found"));
    }

    #[test]
    fn post_hooks_exit_zero_including_branch_and_file_checkouts() {
        let fixture = Fixture::isolated();
        fixture.init();
        for args in [
            &["hooks", "run", "post-merge", "0"][..],
            &["hooks", "run", "post-rewrite", "rebase"][..],
            &["hooks", "run", "post-checkout", "old", "new", "0"][..],
            &["hooks", "run", "post-checkout", "old", "new", "1"][..],
            &["hooks", "run", "post-checkout"][..],
        ] {
            fixture.command().args(args).assert().success();
        }
    }

    #[cfg(unix)]
    #[test]
    fn hook_scripts_prefer_path_and_forward_git_arguments_verbatim() {
        let fixture = Fixture::isolated();
        fixture.init();
        fixture.commit();
        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        let binary = fixture.fake_executable("my-skills");
        fs::write(&binary, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n").unwrap();
        // The hook executes this script indirectly. Check that it can be launched
        // before invoking the hook, where a spawn error would be a shell exit code.
        assert_with_busy_retry(&mut fixture.process(&binary)).success();

        let hook_path = fixture.registry.join(".git/hooks/post-checkout");
        assert_with_busy_retry(fixture.process(hook_path).args([
            "one value",
            "second",
            "1",
            "--example",
        ]))
        .success()
        .stdout("<hooks>\n<run>\n<post-checkout>\n<one value>\n<second>\n<1>\n<--example>\n");
    }

    #[cfg(windows)]
    #[test]
    fn hooks_cli_displays_clean_windows_paths_without_mixed_separators() {
        let fixture = Fixture::isolated();
        fixture.init();

        let assert_clean_paths = |text: &str, prefix: &str| {
            let lines: Vec<_> = text
                .lines()
                .filter_map(|line| line.strip_prefix(prefix))
                .collect();
            assert!(
                !lines.is_empty(),
                "expected lines with prefix {prefix:?} in:\n{text}"
            );
            for path in lines {
                assert!(
                    !path.contains('/'),
                    "displayed path mixes forward slashes on Windows: {path}"
                );
                assert!(
                    !path.contains(r"\\?\"),
                    "displayed path contains verbatim prefix: {path}"
                );
                assert!(
                    path.contains('\\'),
                    "expected Windows separator in displayed path: {path}"
                );
            }
        };

        let dry_install = fixture
            .command()
            .args(["hooks", "install", "--dry-run"])
            .assert()
            .success();
        assert_clean_paths(
            std::str::from_utf8(&dry_install.get_output().stdout).unwrap(),
            "would install: ",
        );

        let install = fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        assert_clean_paths(
            std::str::from_utf8(&install.get_output().stdout).unwrap(),
            "installed: ",
        );

        let status = fixture
            .command()
            .args(["hooks", "status"])
            .assert()
            .success();
        assert_clean_paths(
            std::str::from_utf8(&status.get_output().stdout).unwrap(),
            "  Directory: ",
        );

        let dry_uninstall = fixture
            .command()
            .args(["hooks", "uninstall", "--dry-run"])
            .assert()
            .success();
        assert_clean_paths(
            std::str::from_utf8(&dry_uninstall.get_output().stdout).unwrap(),
            "would remove: ",
        );

        let uninstall = fixture
            .command()
            .args(["hooks", "uninstall"])
            .assert()
            .success();
        assert_clean_paths(
            std::str::from_utf8(&uninstall.get_output().stdout).unwrap(),
            "removed: ",
        );

        fixture.write(".git/hooks/post-rewrite", "#!/bin/sh\necho foreign\n");
        let foreign = fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .code(1);
        let foreign_stderr = std::str::from_utf8(&foreign.get_output().stderr).unwrap();
        let foreign_line = foreign_stderr
            .lines()
            .find(|line| line.starts_with("Foreign hook at "))
            .expect("foreign hook warning line");
        let foreign_path = foreign_line
            .strip_prefix("Foreign hook at ")
            .and_then(|rest| rest.strip_suffix(". Add this line to that hook:"))
            .expect("foreign hook path");
        assert!(
            !foreign_path.contains('/'),
            "foreign hook path mixes forward slashes on Windows: {foreign_path}"
        );
        assert!(
            !foreign_path.contains(r"\\?\"),
            "foreign hook path contains verbatim prefix: {foreign_path}"
        );
        assert!(
            foreign_path.contains('\\'),
            "expected Windows separator in foreign hook path: {foreign_path}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn status_io_error_displays_clean_windows_hook_path() {
        use std::os::windows::fs::OpenOptionsExt;

        let fixture = Fixture::isolated();
        fixture.init();
        fixture.write(".git/hooks/pre-commit", "#!/bin/sh\n");
        let hook = fixture
            .registry
            .join(".git")
            .join("hooks")
            .join("pre-commit");

        // Hold the hook file exclusively so `inspect`'s `File::open` fails with an I/O error.
        let _lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&hook)
            .unwrap();

        for args in [&["hooks", "status"][..], &["hooks", "install"][..]] {
            let result = fixture.command().args(args).assert().code(1);
            let stderr = std::str::from_utf8(&result.get_output().stderr).unwrap();
            let line = stderr
                .lines()
                .find_map(|line| line.strip_prefix("error: "))
                .unwrap_or_else(|| {
                    panic!("expected 'error: ...' line in stderr for {args:?}:\n{stderr}")
                });
            let (path_part, _) = line.split_once(": ").unwrap_or_else(|| {
                panic!("expected '<path>: <source>' in stderr for {args:?}:\n{stderr}")
            });
            assert!(
                !path_part.contains('/'),
                "hook I/O error path mixes forward slashes on Windows ({args:?}): {stderr}"
            );
            assert!(
                !path_part.contains(r"\\?\"),
                "hook I/O error path contains verbatim prefix ({args:?}): {stderr}"
            );
            assert!(
                path_part.ends_with(r"\.git\hooks\pre-commit"),
                "unexpected hook I/O error path ({args:?}): {stderr}"
            );
        }
    }
}

mod v141_hooks {
    use assert_cmd::Command;
    use std::fs::{self, File, Permissions};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::common::{Fixture, MANIFEST, skill_md as skill};
    use assert_cmd::prelude::*;
    use predicates::{prelude::PredicateBooleanExt, str::contains};

    use super::HOOKS;
    const MARKER: &str = "# managed-by: my-skills hooks v1";
    const CUSTOM_SKIP: &str = "Git hooks: skipped (core.hooksPath or a linked hooks folder is set; run 'my-skills hooks install' to install there)";
    const SUBDIR_SKIP: &str =
        "Git hooks: skipped (inside a subdirectory of a larger Git repository)";
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
        fixture.root.join("clone")
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
        let copy = fixture.root.join("copy-host");
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
        let target = fixture.root.join("new-registry");
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
        let target = fixture.root.join("uncommitted");
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
        let no_git = fixture.root.join("no-git");
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

        let no_hooks = fixture.root.join("no-hooks");
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
        let cache = fixture.root.join("config/my-skills/root");
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
        let cache = fixture.root.join("config/my-skills/root");
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
        let target = fixture.root.join("preview");
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
        assert!(!fixture.root.join("config/my-skills/root").exists());

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
        let linked = fixture.root.join("linked");
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
        let dir = fixture.root.join("outside");
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
                predicates::str::contains("warning: my-skills could not add missing hooks: ")
                    .count(1),
            );
        }
        assert_absent(&dir, &["post-commit"]);
    }

    #[test]
    fn bootstrap_warns_and_keeps_sync_results_when_the_hooks_directory_is_a_file() {
        let fixture = committed_registry();
        let copy = set_copy_target(&fixture);
        let dir = hooks(&fixture.registry);
        fs::rename(&dir, fixture.root.join("original-hooks")).unwrap();
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

        fs::rename(&dir, fixture.root.join("original-hooks")).unwrap();
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
        let active = fixture.root.join("outside");
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

        let cache = fixture.root.join("config/my-skills/root");
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
        let trace = fixture.root.join("git-trace.log");
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
                contains("git init failed; skipped initial commit\n")
                    .and(contains("Git hooks:").not()),
            );
        assert_eq!(snapshot_dir(&dir), before);
        assert!(!hooks(&target).exists());

        let outside = fixture.root.join("failed-init-outside");
        init_registry(&fixture, &outside)
            .env("GIT_DEFAULT_HASH", "invalid-hash")
            .assert()
            .success()
            .stdout(
                contains("git init failed; skipped initial commit\n")
                    .and(contains("Git hooks:").not()),
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
        let outside = fixture.root.join("outside");
        fs::write(outside.join("keep.txt"), "keep\n").unwrap();
        let before = snapshot_dir(&outside);
        fs::rename(&dir, fixture.root.join("original-hooks")).unwrap();
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
}

mod w16_post_commit {
    use std::fs::{self, File, OpenOptions};
    use std::path::{Path, PathBuf};
    use std::process::Output;

    use crate::common::Fixture;
    use fs4::FileExt;
    use my_skills::drive::link::{self, EntryKind};
    use my_skills::drive::state::State;
    use my_skills::registry::manifest::InstallMode;
    use predicates::str::contains;

    fn revision(name: &str, rev: u32) -> String {
        format!("---\nname: {name}\ndescription: Revision {rev}.\n---\n\n# Example\n")
    }

    struct Setup {
        fixture: Fixture,
        link_host: PathBuf,
        copy_host: PathBuf,
    }

    impl Setup {
        fn manifest(&self, extra: &str) -> String {
            let path = |host: &Path| serde_json::to_string(host).unwrap();
            format!(
                "schema_version = 1\nskills_root = \"skills\"\n\n\
             [targets.claude]\nenabled = true\npath = {}\ninstall_mode = \"link\"\n\n\
             [targets.codex]\nenabled = true\npath = {}\ninstall_mode = \"copy\"\n\n\
             [targets.agy]\nenabled = false\n\n{extra}",
                path(&self.link_host),
                path(&self.copy_host),
            )
        }

        fn write_manifest(&self, extra: &str) {
            self.fixture.write("my-skills.toml", &self.manifest(extra));
        }

        /// A git registry with two enabled skills, synced to a link and a copy
        /// host, and no hooks installed yet.
        fn synced() -> Self {
            let fixture = Fixture::isolated();
            let setup = Self {
                link_host: fixture.root.join("hosts/claude"),
                copy_host: fixture.root.join("hosts/codex"),
                fixture,
            };
            setup.write_manifest(
                "[skills.alpha]\nenabled = true\n\n[skills.beta]\nenabled = true\n",
            );
            for name in ["alpha", "beta"] {
                setup
                    .fixture
                    .write(&format!("skills/{name}/SKILL.md"), &revision(name, 1));
            }
            setup.fixture.init();
            setup.fixture.commit();
            setup.sync_ok();
            setup
        }

        fn sync_ok(&self) {
            self.fixture
                .command()
                .args(["sync", "--yes", "--quiet"])
                .assert()
                .success();
        }

        fn install_hooks(&self) {
            self.fixture
                .command()
                .args(["hooks", "install"])
                .assert()
                .success();
        }

        fn copy_text(&self, name: &str) -> String {
            fs::read_to_string(self.copy_host.join(name).join("SKILL.md")).unwrap()
        }

        fn link_text(&self, name: &str) -> String {
            fs::read_to_string(self.link_host.join(name).join("SKILL.md")).unwrap()
        }

        fn copy_revision(&self, name: &str) -> u32 {
            revision_of(&self.copy_text(name))
        }

        fn commit_output(&self, args: &[&str]) -> Output {
            let output = self.fixture.git_command(args).output().unwrap();
            // The commit already happened: no post-commit outcome may fail it.
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output
        }

        fn assert_link_host_fresh(&self) {
            self.fixture
                .command()
                .args(["sync", "--check"])
                .assert()
                .success();
        }

        fn run_post_commit(&self) -> assert_cmd::assert::Assert {
            self.fixture
                .command()
                .args(["hooks", "run", "post-commit"])
                .assert()
                .success()
        }
    }

    fn revision_of(text: &str) -> u32 {
        let start = text.find("Revision ").expect("revision marker") + "Revision ".len();
        text[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap()
    }

    fn stderr(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    #[test]
    fn committing_a_skill_change_refreshes_the_copy_host_and_keeps_the_link_host_fresh() {
        let setup = Setup::synced();
        setup.install_hooks();
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "skills"]);
        // The copy host is stale until the commit lands.
        assert_eq!(setup.copy_revision("alpha"), 1);
        let output = setup.commit_output(&["commit", "-qm", "alpha 2"]);

        assert_eq!(setup.copy_revision("alpha"), 2, "{}", stderr(&output));
        assert_eq!(setup.copy_revision("beta"), 1);
        assert_eq!(revision_of(&setup.link_text("alpha")), 2);
        setup.assert_link_host_fresh();
        // Git forwards a hook's stdout to its stderr: the quiet sync reports changes.
        let text = stderr(&output);
        assert!(text.contains("updated: alpha -> codex"), "{text}");
        assert!(!text.contains("warning"), "{text}");
    }

    #[test]
    fn post_commit_keeps_legacy_copies_under_default_link_mode() {
        let setup = Setup::synced();
        let manifest = setup
            .manifest("[skills.alpha]\n[skills.beta]\n")
            .replace("install_mode = \"copy\"\n", "")
            .replace(
                "[targets.claude]",
                "[defaults]\ninstall_mode = 'link'\n\n[targets.claude]",
            );
        setup.install_hooks();
        setup.fixture.write("my-skills.toml", &manifest);
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "my-skills.toml", "skills"]);
        let output = setup.commit_output(&["commit", "-qm", "Refresh a legacy copy"]);

        assert_eq!(setup.copy_revision("alpha"), 2, "{}", stderr(&output));
        let dest = setup.copy_host.join("alpha");
        assert_eq!(link::entry_kind(&dest).unwrap(), EntryKind::Dir);
        let state_path = setup.fixture.root.join("state/my-skills/state.json");
        let state = State::load(Some(&state_path)).unwrap();
        let record = state.get("alpha", "codex").unwrap();
        assert_eq!(record.mode, InstallMode::Copy);
        assert_eq!(record.fallback_reason, None);
        setup.assert_link_host_fresh();
    }

    #[test]
    fn a_dirty_skill_is_skipped_with_one_warning_while_clean_skills_refresh() {
        let setup = Setup::synced();
        setup.install_hooks();
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup
            .fixture
            .write("skills/beta/SKILL.md", &revision("beta", 2));
        setup.fixture.git(&["add", "skills"]);
        // An untracked file keeps alpha dirty after the commit.
        setup
            .fixture
            .write("skills/alpha/scratch.md", "work in progress\n");
        let output = setup.commit_output(&["commit", "-qm", "both"]);

        let text = stderr(&output);
        assert_eq!(setup.copy_revision("alpha"), 1, "{text}");
        assert_eq!(setup.copy_revision("beta"), 2, "{text}");
        assert_eq!(text.matches("warning:").count(), 1, "{text}");
        assert!(
            text.contains("warning: post-commit left alpha alone"),
            "{text}"
        );
        assert!(text.contains("updated: beta -> codex"), "{text}");
        assert!(!text.contains("updated: alpha"), "{text}");
    }

    #[test]
    fn a_manifest_change_runs_a_full_sync_that_leaves_dirty_skills_alone() {
        let setup = Setup::synced();
        setup.install_hooks();
        // alpha has an uncommitted edit; a full sync must neither connect nor
        // prune it. beta changes in a way only a full sync would notice later.
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup
            .fixture
            .write("skills/gamma/SKILL.md", &revision("gamma", 1));
        setup.write_manifest(
            "[skills.alpha]\nenabled = true\n\n[skills.beta]\nenabled = true\n\n\
         [skills.gamma]\nenabled = true\n",
        );
        setup
            .fixture
            .git(&["add", "my-skills.toml", "skills/gamma"]);
        let output = setup.commit_output(&["commit", "-qm", "add gamma"]);

        let text = stderr(&output);
        assert_eq!(revision_of(&setup.copy_text("gamma")), 1, "{text}");
        assert_eq!(setup.copy_revision("alpha"), 1, "{text}");
        assert!(
            text.contains("warning: post-commit left alpha alone"),
            "{text}"
        );
        // Once alpha is committed, the next commit brings it up to date.
        setup.fixture.git(&["add", "skills/alpha"]);
        setup.commit_output(&["commit", "-qm", "alpha 2"]);
        assert_eq!(setup.copy_revision("alpha"), 2);
    }

    #[test]
    fn a_full_sync_does_not_prune_a_dirty_skill_removed_from_hosts_by_the_manifest() {
        let setup = Setup::synced();
        // alpha is disabled in the commit, which would prune it, but its files
        // have uncommitted edits.
        setup.write_manifest("[skills.alpha]\nenabled = false\n\n[skills.beta]\nenabled = true\n");
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "my-skills.toml"]);
        setup.fixture.git(&["commit", "-qm", "disable alpha"]);
        setup.run_post_commit().stderr(contains("left alpha alone"));
        assert!(setup.copy_host.join("alpha/SKILL.md").is_file());
        // Once the skill is clean, the next full sync prunes it.
        setup.fixture.git(&["checkout", "--", "skills/alpha"]);
        setup.run_post_commit().stderr("");
        assert!(!setup.copy_host.join("alpha").exists());
        assert_eq!(setup.copy_revision("beta"), 1);
    }

    #[test]
    fn an_uncommitted_manifest_change_skips_everything_with_one_warning() {
        let setup = Setup::synced();
        setup.install_hooks();
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "skills"]);
        // Unstaged manifest edit.
        let manifest =
            setup.manifest("[skills.alpha]\nenabled = true\n\n[skills.beta]\nenabled = true\n");
        setup
            .fixture
            .write("my-skills.toml", &format!("{manifest}# note\n"));
        let output = setup.commit_output(&["commit", "-qm", "alpha 2", "--", "skills"]);
        let text = stderr(&output);
        assert_eq!(setup.copy_revision("alpha"), 1, "{text}");
        assert_eq!(
            text.matches("my-skills.toml has uncommitted changes")
                .count(),
            1,
            "{text}"
        );

        // The same with the manifest edit staged but left out of the commit.
        setup.fixture.git(&["add", "my-skills.toml"]);
        setup
            .fixture
            .write("skills/beta/SKILL.md", &revision("beta", 2));
        setup.commit_output(&["commit", "-qm", "beta 2", "--", "skills/beta"]);
        assert_eq!(setup.copy_revision("beta"), 1);
        // A machine-local manifest is untracked on purpose and never blocks.
        setup.fixture.git(&["commit", "-qm", "manifest"]);
        setup.fixture.write(
            "my-skills.local.toml",
            "[defaults]\nverify_after_install = true\n",
        );
        setup
            .fixture
            .write("skills/beta/SKILL.md", &revision("beta", 3));
        setup.fixture.git(&["add", "skills"]);
        let output = setup.commit_output(&["commit", "-qm", "beta 3"]);
        assert_eq!(setup.copy_revision("beta"), 3, "{}", stderr(&output));
    }

    #[test]
    fn a_commit_in_a_linked_worktree_is_a_no_op_for_every_post_hook() {
        let setup = Setup::synced();
        setup.install_hooks();
        let worktree = setup.fixture.root.join("linked");
        let plain = worktree.to_str().unwrap().trim_start_matches(r"\\?\");
        setup
            .fixture
            .git(&["worktree", "add", "-q", "-b", "other", plain]);
        fs::write(worktree.join("skills/alpha/SKILL.md"), revision("alpha", 2)).unwrap();
        let in_worktree = |args: &[&str]| {
            let mut command = setup.fixture.git_command(args);
            command.current_dir(&worktree);
            command.output().unwrap()
        };
        assert!(in_worktree(&["add", "skills"]).status.success());
        let output = in_worktree(&["commit", "-qm", "alpha 2 in the worktree"]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(setup.copy_revision("alpha"), 1, "{}", stderr(&output));
        assert_eq!(stderr(&output), "");

        // The other post hooks get the same relative registry path as the hook
        // script exports; they must not sync from the worktree either.
        fs::write(worktree.join("skills/alpha/SKILL.md"), revision("alpha", 3)).unwrap();
        for (hook, args) in [
            ("post-commit", &[][..]),
            ("post-merge", &["0"][..]),
            ("post-rewrite", &["amend"][..]),
            ("post-checkout", &["a", "b", "1"][..]),
        ] {
            let mut command = setup.fixture.command();
            command
                .current_dir(&worktree)
                .env("MY_SKILLS_ROOT", ".")
                .args(["hooks", "run", hook])
                .args(args);
            command.assert().success().stderr("");
            assert_eq!(setup.copy_revision("alpha"), 1, "{hook}");
        }

        // The same hook from the main checkout does sync (the guard is the only
        // difference), here the checkout still has revision 1 committed.
        fs::write(
            setup.fixture.registry.join("skills/alpha/SKILL.md"),
            revision("alpha", 2),
        )
        .unwrap();
        setup.fixture.git(&["add", "skills"]);
        setup.fixture.git(&["commit", "-qm", "alpha 2"]);
        setup.run_post_commit();
        assert_eq!(setup.copy_revision("alpha"), 2);
    }

    #[test]
    fn a_rebase_merge_or_cherry_pick_in_progress_is_a_no_op() {
        let setup = Setup::synced();
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "skills"]);
        setup.fixture.git(&["commit", "-qm", "alpha 2"]);
        setup.install_hooks();
        let git_dir = setup.fixture.registry.join(".git");
        for marker in [
            "rebase-merge",
            "rebase-apply",
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
        ] {
            let path = git_dir.join(marker);
            if marker.ends_with("HEAD") {
                fs::write(&path, "0000000000000000000000000000000000000000\n").unwrap();
            } else {
                fs::create_dir(&path).unwrap();
            }
            setup.run_post_commit().stderr("");
            assert_eq!(setup.copy_revision("alpha"), 1, "{marker}");
            if path.is_dir() {
                fs::remove_dir(&path).unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
        }
        // Without a marker the same call refreshes the copy host.
        setup.run_post_commit();
        assert_eq!(setup.copy_revision("alpha"), 2);
    }

    fn hold_state_lock(fixture: &Fixture) -> File {
        let lock = fixture.root.join("state/my-skills/state.json.lock");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock)
            .unwrap();
        FileExt::lock(&file).unwrap();
        file
    }

    #[test]
    fn a_busy_state_lock_warns_and_still_exits_zero() {
        let setup = Setup::synced();
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "skills"]);
        setup.fixture.git(&["commit", "-qm", "alpha 2"]);
        let lock = hold_state_lock(&setup.fixture);
        setup
            .fixture
            .command()
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "5")
            .args(["hooks", "run", "post-commit"])
            .assert()
            .success()
            .stderr(contains("state is busy"))
            .stderr(contains("post-commit could not sync the registry"));
        assert_eq!(setup.copy_revision("alpha"), 1);
        drop(lock);
        setup.run_post_commit().stderr("");
        assert_eq!(setup.copy_revision("alpha"), 2);
    }

    fn status_table(fixture: &Fixture) -> String {
        let output = fixture
            .command()
            .args(["hooks", "status"])
            .assert()
            .success();
        String::from_utf8_lossy(&output.get_output().stdout)
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    #[test]
    fn hooks_status_lists_post_commit_and_flags_an_old_install_as_missing() {
        let fixture = Fixture::isolated();
        fixture.init();
        let hooks = fixture.registry.join(".git/hooks");
        for name in ["pre-commit", "post-merge", "post-rewrite", "post-checkout"] {
            fixture.write(
                &format!(".git/hooks/{name}"),
                "#!/bin/sh
# managed-by: my-skills hooks v1
exit 0
",
            );
        }
        let table = status_table(&fixture);
        assert!(table.contains("post-commit missing -"), "{table}");
        assert!(table.contains("post-merge installed v1"), "{table}");
        assert!(!hooks.join("post-commit").exists());

        fixture
            .command()
            .args(["hooks", "install"])
            .assert()
            .success();
        let table = status_table(&fixture);
        assert!(table.contains("post-commit installed v1"), "{table}");
        let script = fs::read_to_string(hooks.join("post-commit")).unwrap();
        assert!(script.contains("hooks run post-commit \"$@\""), "{script}");
        fixture
            .command()
            .args(["hooks", "uninstall"])
            .assert()
            .success();
        assert!(!hooks.join("post-commit").exists());
    }

    #[test]
    fn the_root_commit_runs_a_full_sync() {
        let fixture = Fixture::isolated();
        let setup = Setup {
            link_host: fixture.root.join("hosts/claude"),
            copy_host: fixture.root.join("hosts/codex"),
            fixture,
        };
        setup.write_manifest("[skills.alpha]\nenabled = true\n");
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 1));
        setup.fixture.init();
        setup.install_hooks();
        setup.fixture.git(&["add", "."]);
        let output = setup.commit_output(&["commit", "-qm", "root"]);
        assert_eq!(setup.copy_revision("alpha"), 1, "{}", stderr(&output));
        assert_eq!(revision_of(&setup.link_text("alpha")), 1);
        setup.assert_link_host_fresh();

        // A root commit that only adds an unregistered draft does nothing.
        let draft = Fixture::isolated();
        let other = Setup {
            link_host: draft.root.join("hosts/claude"),
            copy_host: draft.root.join("hosts/codex"),
            fixture: draft,
        };
        other.write_manifest("");
        other
            .fixture
            .write("skills/sketch/SKILL.md", &revision("sketch", 1));
        other.fixture.init();
        other.install_hooks();
        other.fixture.git(&["add", "."]);
        let output = other.commit_output(&["commit", "-qm", "draft"]);
        assert_eq!(stderr(&output), "");
        assert!(!other.copy_host.exists());
    }

    fn recorded(fixture: &Fixture, skill: &str) -> bool {
        let state = my_skills::drive::state::State::load(Some(
            &fixture.root.join("state/my-skills/state.json"),
        ))
        .unwrap();
        state.records().any(|record| record.skill == skill)
    }

    #[test]
    fn a_dirty_skill_the_commit_unregisters_is_not_pruned() {
        let setup = Setup::synced();
        // The commit removes alpha from the manifest; alpha has an uncommitted edit.
        setup.write_manifest("[skills.beta]\nenabled = true\n");
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "my-skills.toml"]);
        setup.fixture.git(&["commit", "-qm", "unregister alpha"]);
        setup.run_post_commit().stderr(contains("left alpha alone"));
        assert!(setup.copy_host.join("alpha/SKILL.md").is_file());
        assert!(setup.link_host.join("alpha").exists());
        assert!(recorded(&setup.fixture, "alpha"));

        // Once the edit is committed or dropped, the next full sync prunes it.
        setup.fixture.git(&["checkout", "--", "skills/alpha"]);
        setup.run_post_commit().stderr("");
        assert!(!setup.copy_host.join("alpha").exists());
        assert!(!recorded(&setup.fixture, "alpha"));
        assert_eq!(setup.copy_revision("beta"), 1);
    }

    #[test]
    fn an_unregistered_draft_with_no_connection_is_not_reported() {
        let setup = Setup::synced();
        setup
            .fixture
            .write("skills/sketch/SKILL.md", &revision("sketch", 1));
        setup.write_manifest(
            "[skills.alpha]\nenabled = true\n\n[skills.beta]\nenabled = true\n\n# touched\n",
        );
        setup.fixture.git(&["add", "my-skills.toml"]);
        setup.fixture.git(&["commit", "-qm", "manifest"]);
        setup.install_hooks();
        setup.run_post_commit().stderr("");
    }

    #[test]
    fn edits_made_while_the_hook_waits_for_the_state_lock_are_not_copied() {
        use std::io::{BufRead, BufReader};
        use std::process::Stdio;

        let setup = Setup::synced();
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "skills"]);
        setup.fixture.git(&["commit", "-qm", "alpha 2"]);
        let lock = hold_state_lock(&setup.fixture);
        let mut hook = setup
            .fixture
            .process(assert_cmd::cargo::cargo_bin!("my-skills"));
        let mut child = hook
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "30000")
            .args(["hooks", "run", "post-commit"])
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let mut seen = String::new();
        loop {
            let mut line = String::new();
            assert!(stderr.read_line(&mut line).unwrap() > 0, "{seen}");
            seen.push_str(&line);
            if line.contains("Waiting for another my-skills command") {
                break;
            }
        }
        // The hook is now blocked on the lock, before it has looked at git.
        setup
            .fixture
            .write("skills/alpha/scratch.md", "uncommitted work\n");
        drop(lock);
        let mut rest = String::new();
        std::io::Read::read_to_string(&mut stderr, &mut rest).unwrap();
        assert!(child.wait().unwrap().success());
        assert!(rest.contains("left alpha alone"), "{rest}");
        assert_eq!(setup.copy_revision("alpha"), 1);
        assert!(!setup.copy_host.join("alpha/scratch.md").exists());
    }

    #[test]
    fn a_conflict_resolved_merge_commit_refreshes_the_copy_host() {
        let setup = Setup::synced();
        setup.fixture.git(&["branch", "other"]);
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 2));
        setup.fixture.git(&["add", "skills"]);
        setup.fixture.git(&["commit", "-qm", "alpha 2"]);
        setup.fixture.git(&["checkout", "-q", "other"]);
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 3));
        setup.fixture.git(&["add", "skills"]);
        setup.fixture.git(&["commit", "-qm", "alpha 3"]);
        setup.fixture.git(&["checkout", "-q", "main"]);
        assert_eq!(setup.copy_revision("alpha"), 1);
        setup.install_hooks();

        let merge = setup
            .fixture
            .git_command(&["merge", "other"])
            .output()
            .unwrap();
        assert!(!merge.status.success(), "the merge should conflict");
        setup
            .fixture
            .write("skills/alpha/SKILL.md", &revision("alpha", 4));
        setup.fixture.git(&["add", "skills"]);
        let output = setup.commit_output(&["commit", "--no-edit", "-q"]);
        assert_eq!(setup.copy_revision("alpha"), 4, "{}", stderr(&output));
        setup.assert_link_host_fresh();
    }
}
