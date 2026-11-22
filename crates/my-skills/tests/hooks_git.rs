#[allow(dead_code)]
mod support_g;

use std::fs;

use assert_cmd::prelude::*;
use predicates::str::contains;
use support_g::{Fixture, MANIFEST, skill};

const HOOKS: [&str; 4] = ["pre-commit", "post-merge", "post-rewrite", "post-checkout"];

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
fn a_foreign_hook_blocks_every_install_write_and_prints_the_manual_command() {
    let fixture = Fixture::isolated();
    fixture.init();
    fixture.write(".git/hooks/post-rewrite", "#!/bin/sh\necho foreign\n");
    fixture.write(
        ".git/hooks/pre-commit",
        "#!/bin/sh\n# managed-by: my-skills hooks v0\n",
    );
    for args in [
        &["hooks", "install"][..],
        &["hooks", "install", "--dry-run"][..],
    ] {
        fixture
            .command()
            .args(args)
            .assert()
            .code(1)
            .stderr(contains("my-skills hooks run post-rewrite \"$@\""));
    }
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
    assert_eq!(
        fs::read_to_string(fixture.registry.join(".git/hooks/pre-commit")).unwrap(),
        "#!/bin/sh\n# managed-by: my-skills hooks v0\n"
    );
    assert!(!fixture.registry.join(".git/hooks/post-merge").exists());
}

#[cfg(unix)]
#[test]
fn links_are_neither_managed_hooks_nor_exportable_index_files() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::isolated();
    fixture.init();
    let payload = fixture.base.join("external-file");
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
        .base
        .join(format!("cli's copy{}", std::env::consts::EXE_SUFFIX));
    fs::copy(assert_cmd::cargo::cargo_bin("my-skills"), &binary).unwrap();
    fixture
        .command_from(&binary)
        .args(["hooks", "install"])
        .assert()
        .success();

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

    fixture.git(&["add", "skills"]);
    fixture.write("skills/example/SKILL.md", &skill("example"));
    fixture
        .git_command(&["commit", "-qm", "invalid index"])
        .assert()
        .code(1)
        .stderr(contains("[BLOCKED] example:"));
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
    fixture.write("skills/example/SKILL.md", "broken staged skill\n");
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
        .stderr(contains("[BLOCKED] example:"));
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
fn pre_commit_validates_staged_manifest_layout_host_limits_and_audit() {
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
        "a".repeat(800)
    );
    fixture.write("training material/example/SKILL.md", &long);
    fixture.write("training material/second/SKILL.md", &skill("second"));
    fixture.write(
        "training material/second/notes.txt",
        "-----BEGIN PRIVATE KEY-----\nexample\n-----END PRIVATE KEY-----\n",
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
        .stderr(contains(
            "[BLOCKED] example: codex description exceeds 768 characters",
        ))
        .stderr(contains("[BLOCKED] second: security: notes.txt:"));

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
        "a".repeat(800)
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
    fixture.write("skills/example/SKILL.md", "invalid staged content\n");
    fixture.registry = top;
    fixture.git(&["add", "."]);
    fixture
        .git_command(&["commit", "-qm", "initial"])
        .assert()
        .code(1)
        .stderr(contains("[BLOCKED] example:"));
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
    fs::write(binary, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n").unwrap();

    let hook_path = fixture.registry.join(".git/hooks/post-checkout");
    fixture
        .command_from(hook_path)
        .args(["one value", "second", "1", "--example"])
        .assert()
        .success()
        .stdout("<hooks>\n<run>\n<post-checkout>\n<one value>\n<second>\n<1>\n<--example>\n");
}
