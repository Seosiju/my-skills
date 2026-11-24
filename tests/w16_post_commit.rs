#[allow(dead_code)]
mod support_g;

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::assert::OutputAssertExt;
use fs4::FileExt;
use predicates::str::contains;
use support_g::Fixture;

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
            link_host: fixture.base.join("hosts/claude"),
            copy_host: fixture.base.join("hosts/codex"),
            fixture,
        };
        setup.write_manifest("[skills.alpha]\nenabled = true\n\n[skills.beta]\nenabled = true\n");
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
    let worktree = setup.fixture.base.join("linked");
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
    let lock = fixture.base.join("xdg-state/my-skills/state.json.lock");
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
        link_host: fixture.base.join("hosts/claude"),
        copy_host: fixture.base.join("hosts/codex"),
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
        link_host: draft.base.join("hosts/claude"),
        copy_host: draft.base.join("hosts/codex"),
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
        &fixture.base.join("xdg-state/my-skills/state.json"),
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
    let mut hook = setup.fixture.command();
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
