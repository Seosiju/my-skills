//! End-to-end drive behavior on a temporary registry and host directories.
//! Spec §3, D1 (link default), D3 (reconcile with prune), invariants I1, I3, I5, I7.

use std::fs;
use std::path::{Path, PathBuf};

use my_skills::drive::apply::{self, Connection, Outcome};
use my_skills::drive::link::{self, EntryKind};
use my_skills::drive::plan::{self, Action, Status, SyncScope};
use my_skills::drive::state::State;
use my_skills::registry::manifest::{
    CliOverrides, InstallMode, LOCAL_MANIFEST_FILE, Manifest, load_manifest,
};

const HOSTS: [&str; 3] = ["agy", "claude", "codex"];

struct Drive {
    _tmp: tempfile::TempDir,
    registry: PathBuf,
    hosts_root: PathBuf,
    state_path: PathBuf,
}

impl Drive {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let registry = tmp.path().join("registry");
        let hosts_root = tmp.path().join("hosts");
        for skill in ["alpha", "beta"] {
            let dir = registry.join("skills").join(skill);
            fs::create_dir_all(dir.join("scripts")).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {skill}\ndescription: Demo skill {skill}.\n---\nBody\n"),
            )
            .unwrap();
            fs::write(dir.join("scripts").join("run.sh"), "echo hi\n").unwrap();
        }
        let drive = Drive {
            state_path: tmp.path().join("state").join("state.json"),
            _tmp: tmp,
            registry,
            hosts_root,
        };
        drive.write_manifest(true, true, "link");
        drive
    }

    fn host_dir(&self, host: &str) -> PathBuf {
        self.hosts_root.join(host)
    }

    fn write_manifest(&self, alpha_enabled: bool, beta_registered: bool, mode: &str) {
        let mut toml = format!(
            "schema_version = 1\nskills_root = \"skills\"\n\n[defaults]\ninstall_mode = \"{mode}\"\n"
        );
        for host in HOSTS {
            toml.push_str(&format!(
                "\n[targets.{host}]\nenabled = true\npath = \"{}\"\n",
                self.host_dir(host)
                    .display()
                    .to_string()
                    .replace('\\', "\\\\")
            ));
        }
        toml.push_str(&format!(
            "\n[skills.alpha]\nenabled = {alpha_enabled}\nhosts = [\"claude\", \"codex\", \"agy\", \"custom\"]\n"
        ));
        if beta_registered {
            toml.push_str(
                "\n[skills.beta]\nenabled = true\nhosts = [\"claude\", \"codex\", \"agy\"]\n",
            );
        }
        fs::write(self.registry.join("my-skills.toml"), toml).unwrap();
    }

    fn write_custom_target(&self, enabled: bool) {
        let path = serde_json::to_string(&self.host_dir("custom")).unwrap();
        fs::write(
            self.registry.join(LOCAL_MANIFEST_FILE),
            format!("[targets.custom]\nenabled = {enabled}\npath = {path}\n"),
        )
        .unwrap();
    }

    fn manifest(&self) -> Manifest {
        load_manifest(&self.registry, &CliOverrides::default()).unwrap()
    }

    fn state(&self) -> State {
        State::load(Some(&self.state_path)).unwrap()
    }

    /// Plan and apply a full sync; returns the applied plan.
    fn sync(&self, prune: bool) -> Vec<plan::PlanItem> {
        let manifest = self.manifest();
        let mut state = self.state();
        let skills: Vec<_> = manifest.skills.values().collect();
        let hosts: Vec<_> = manifest.targets.keys().cloned().collect();
        let scope = SyncScope {
            skills: &skills,
            hosts: &hosts,
            include_orphans: true,
            prune,
        };
        let items = plan::plan_sync(&manifest, &state, &scope, None).unwrap();
        for item in &items {
            assert!(!item.action.is_block(), "unexpected block: {item:?}");
            apply::apply(item, &mut state, None).unwrap();
        }
        items
    }

    fn status(&self, skill: &str, host: &str) -> Status {
        let manifest = self.manifest();
        let skill = manifest.skills.get(skill).unwrap();
        plan::status_of(&manifest, skill, host, &self.state()).unwrap()
    }
}

fn actions(items: &[plan::PlanItem]) -> Vec<(String, String, Action)> {
    let mut out: Vec<_> = items
        .iter()
        .map(|i| (i.skill.clone(), i.host.clone(), i.action))
        .collect();
    out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    out
}

fn assert_canonical_intact(registry: &Path, skill: &str) {
    let dir = registry.join("skills").join(skill);
    assert!(dir.join("SKILL.md").is_file(), "I1: {skill}/SKILL.md lost");
    assert!(
        dir.join("scripts").join("run.sh").is_file(),
        "I1: {skill} script lost"
    );
}

#[test]
fn first_sync_links_every_wanted_pair_and_skips_disabled_target() {
    let drive = Drive::new();
    drive.write_custom_target(false);
    let items = drive.sync(true);

    let mut expected = Vec::new();
    for skill in ["alpha", "beta"] {
        for host in ["agy", "claude", "codex"] {
            expected.push((skill.to_string(), host.to_string(), Action::Create));
        }
    }
    assert_eq!(
        actions(&items),
        expected,
        "the custom target is disabled and must be skipped"
    );
    for item in &items {
        assert_eq!(item.mode, InstallMode::Link);
        assert!(matches!(
            link::entry_kind(&item.destination).unwrap(),
            EntryKind::Link(_)
        ));
    }
    assert_eq!(drive.status("alpha", "claude"), Status::Fresh);
    assert!(!drive.host_dir("custom").join("alpha").exists());
}

#[test]
fn canonical_edit_is_visible_everywhere_without_sync() {
    let drive = Drive::new();
    drive.sync(true);
    let skill_md = drive.registry.join("skills").join("alpha").join("SKILL.md");
    fs::write(
        &skill_md,
        "---\nname: alpha\ndescription: Edited.\n---\nNew body\n",
    )
    .unwrap();

    for host in ["agy", "claude", "codex"] {
        let seen = fs::read_to_string(drive.host_dir(host).join("alpha").join("SKILL.md")).unwrap();
        assert!(seen.contains("New body"), "{host} does not see the edit");
    }
    let again = drive.sync(true);
    assert!(again.iter().all(|i| i.action == Action::Noop), "{again:?}");
}

#[test]
fn editing_through_a_host_path_edits_the_canonical() {
    let drive = Drive::new();
    drive.sync(true);
    let via_host = drive.host_dir("codex").join("beta").join("notes.md");
    fs::write(&via_host, "written through codex\n").unwrap();
    let canonical = drive.registry.join("skills").join("beta").join("notes.md");
    assert_eq!(
        fs::read_to_string(canonical).unwrap(),
        "written through codex\n"
    );
    assert_eq!(
        drive.status("beta", "claude"),
        Status::Fresh,
        "no drift with links"
    );
}

#[test]
fn disabling_a_skill_prunes_its_links_but_keeps_the_canonical() {
    let drive = Drive::new();
    drive.sync(true);
    drive.write_manifest(false, true, "link");
    let items = drive.sync(true);

    let removed: Vec<_> = actions(&items)
        .into_iter()
        .filter(|(_, _, a)| *a == Action::Remove)
        .collect();
    assert_eq!(removed.len(), 3, "{items:?}");
    assert!(removed.iter().all(|(skill, _, _)| skill == "alpha"));
    for host in ["agy", "claude", "codex"] {
        assert_eq!(
            link::entry_kind(&drive.host_dir(host).join("alpha")).unwrap(),
            EntryKind::Missing
        );
        assert!(drive.host_dir(host).join("beta").join("SKILL.md").is_file());
    }
    assert_canonical_intact(&drive.registry, "alpha");
    assert!(drive.state().get("alpha", "claude").is_none());
}

#[test]
fn re_enabling_restores_the_links() {
    let drive = Drive::new();
    drive.sync(true);
    drive.write_manifest(false, true, "link");
    drive.sync(true);
    drive.write_manifest(true, true, "link");
    drive.sync(true);
    assert_eq!(drive.status("alpha", "agy"), Status::Fresh);
}

#[test]
fn no_prune_keeps_links_of_disabled_skills() {
    let drive = Drive::new();
    drive.sync(true);
    drive.write_manifest(false, true, "link");
    let items = drive.sync(false);
    assert!(
        items.iter().all(|i| i.action != Action::Remove),
        "{items:?}"
    );
    assert!(
        drive
            .host_dir("claude")
            .join("alpha")
            .join("SKILL.md")
            .is_file()
    );
}

#[test]
fn unregistered_skill_is_pruned_as_an_orphan() {
    let drive = Drive::new();
    drive.sync(true);
    drive.write_manifest(true, false, "link");
    let items = drive.sync(true);
    let removed: Vec<_> = actions(&items)
        .into_iter()
        .filter(|(_, _, a)| *a == Action::Remove)
        .map(|(skill, host, _)| (skill, host))
        .collect();
    assert_eq!(
        removed,
        vec![
            ("beta".to_string(), "agy".to_string()),
            ("beta".to_string(), "claude".to_string()),
            ("beta".to_string(), "codex".to_string()),
        ]
    );
    assert_canonical_intact(&drive.registry, "beta");
}

#[test]
fn deleted_canonical_shows_broken_and_is_pruned() {
    let drive = Drive::new();
    drive.sync(true);
    fs::remove_dir_all(drive.registry.join("skills").join("beta")).unwrap();
    assert_eq!(drive.status("beta", "claude"), Status::Broken);

    drive.write_manifest(true, false, "link");
    drive.sync(true);
    assert_eq!(
        link::entry_kind(&drive.host_dir("claude").join("beta")).unwrap(),
        EntryKind::Missing
    );
}

#[test]
fn disabled_and_missing_target_installs_are_never_touched() {
    // I7: a managed copy must survive while its target is disabled or absent.
    let drive = Drive::new();
    drive.write_custom_target(true);
    let manifest = drive.manifest();
    let mut state = drive.state();
    let skill = &manifest.skills["alpha"];
    let item =
        plan::plan_install_one(&manifest, skill, "custom", &state, InstallMode::Copy).unwrap();
    assert_eq!(item.action, Action::Create);
    apply::apply(&item, &mut state, None).unwrap();
    let record = state.get("alpha", "custom").unwrap();
    let copy_path = drive.host_dir("custom").join("alpha/SKILL.md");
    let contents = fs::read(&copy_path).unwrap();

    drive.write_manifest(false, false, "link");
    drive.write_custom_target(false);
    let assert_copy_survives_sync = || {
        let items = drive.sync(true);
        assert!(items.iter().all(|i| i.host != "custom"), "{items:?}");
        assert_eq!(fs::read(&copy_path).unwrap(), contents);
        assert_eq!(drive.state().get("alpha", "custom"), Some(record));
    };
    assert_copy_survives_sync();

    fs::remove_file(drive.registry.join(LOCAL_MANIFEST_FILE)).unwrap();
    assert!(!drive.manifest().targets.contains_key("custom"));
    assert_copy_survives_sync();
}

#[test]
fn unmanaged_destination_blocks_and_is_left_alone() {
    let drive = Drive::new();
    let foreign = drive.host_dir("claude").join("alpha");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("SKILL.md"), "someone else's skill").unwrap();

    let manifest = drive.manifest();
    let state = drive.state();
    let skill = manifest.skills.get("alpha").unwrap();
    let item =
        plan::plan_install_one(&manifest, skill, "claude", &state, InstallMode::Link).unwrap();
    assert_eq!(item.action, Action::BlockConflict);
    assert_eq!(
        fs::read_to_string(foreign.join("SKILL.md")).unwrap(),
        "someone else's skill"
    );
}

#[test]
fn copy_mode_install_then_convert_to_link() {
    let drive = Drive::new();
    drive.write_manifest(true, true, "copy");
    let items = drive.sync(true);
    assert!(items.iter().all(|i| i.mode == InstallMode::Copy));
    assert_eq!(
        link::entry_kind(&drive.host_dir("claude").join("alpha")).unwrap(),
        EntryKind::Dir
    );
    assert_eq!(drive.status("alpha", "claude"), Status::Fresh);

    // A canonical edit makes copies STALE; STALE copies may still convert.
    fs::write(
        drive.registry.join("skills").join("alpha").join("extra.md"),
        "new\n",
    )
    .unwrap();
    assert_eq!(drive.status("alpha", "claude"), Status::Stale);

    let manifest = drive.manifest();
    let mut state = drive.state();
    let skills: Vec<_> = manifest.skills.values().collect();
    let hosts: Vec<String> = ["agy", "claude", "codex"]
        .iter()
        .map(|h| h.to_string())
        .collect();
    let plan = plan::plan_convert(&manifest, &state, &skills, &hosts, InstallMode::Link).unwrap();
    assert!(plan.iter().all(|i| i.action == Action::Convert), "{plan:?}");
    for item in &plan {
        let outcome = apply::apply(item, &mut state, None).unwrap();
        assert!(matches!(outcome, Outcome::Converted(Connection::Link(_))));
    }
    assert_eq!(drive.status("alpha", "claude"), Status::Fresh);
    assert!(
        drive
            .host_dir("claude")
            .join("alpha")
            .join("extra.md")
            .is_file()
    );
    assert_canonical_intact(&drive.registry, "alpha");
}

#[test]
fn drifted_copy_blocks_convert_and_prune() {
    let drive = Drive::new();
    drive.write_manifest(true, true, "copy");
    drive.sync(true);
    let host_copy = drive.host_dir("codex").join("alpha").join("SKILL.md");
    fs::write(&host_copy, "edited in the host copy").unwrap();
    assert_eq!(drive.status("alpha", "codex"), Status::Drifted);

    let manifest = drive.manifest();
    let state = drive.state();
    let skills: Vec<_> = manifest.skills.values().collect();
    let hosts = vec!["codex".to_string()];
    let plan = plan::plan_convert(&manifest, &state, &skills, &hosts, InstallMode::Link).unwrap();
    let alpha = plan.iter().find(|i| i.skill == "alpha").unwrap();
    assert_eq!(alpha.action, Action::BlockDrift);

    drive.write_manifest(false, true, "copy");
    let manifest = drive.manifest();
    let skills: Vec<_> = manifest.skills.values().collect();
    let scope = SyncScope {
        skills: &skills,
        hosts: &hosts,
        include_orphans: true,
        prune: true,
    };
    let items = plan::plan_sync(&manifest, &state, &scope, Some(InstallMode::Link)).unwrap();
    let alpha = items.iter().find(|i| i.skill == "alpha").unwrap();
    assert_eq!(
        alpha.action,
        Action::BlockDrift,
        "I5: local edits are not pruned"
    );
    assert_eq!(
        fs::read_to_string(&host_copy).unwrap(),
        "edited in the host copy"
    );
}

#[test]
fn sync_uses_per_target_install_mode_and_retains_recorded_mode() {
    let drive = Drive::new();
    fs::write(
        drive.registry.join("my-skills.local.toml"),
        "[targets.agy]\ninstall_mode = \"copy\"\n",
    )
    .unwrap();

    let items = drive.sync(true);
    let agy_alpha = items
        .iter()
        .find(|i| i.skill == "alpha" && i.host == "agy")
        .unwrap();
    let claude_alpha = items
        .iter()
        .find(|i| i.skill == "alpha" && i.host == "claude")
        .unwrap();
    assert_eq!(agy_alpha.mode, InstallMode::Copy);
    assert_eq!(claude_alpha.mode, InstallMode::Link);
    assert_eq!(
        link::entry_kind(&drive.host_dir("agy").join("alpha")).unwrap(),
        EntryKind::Dir
    );
    assert!(matches!(
        link::entry_kind(&drive.host_dir("claude").join("alpha")).unwrap(),
        EntryKind::Link(_)
    ));

    // Changing target install_mode in the manifest does not silently convert existing records.
    fs::write(
        drive.registry.join("my-skills.local.toml"),
        "[targets.agy]\ninstall_mode = \"link\"\n",
    )
    .unwrap();
    let again = drive.sync(true);
    let agy_alpha_again = again
        .iter()
        .find(|i| i.skill == "alpha" && i.host == "agy")
        .unwrap();
    assert_eq!(agy_alpha_again.action, Action::Noop);
    assert_eq!(agy_alpha_again.mode, InstallMode::Copy);
    assert_eq!(
        link::entry_kind(&drive.host_dir("agy").join("alpha")).unwrap(),
        EntryKind::Dir
    );
    assert_eq!(
        drive.state().get("alpha", "agy").unwrap().mode,
        InstallMode::Copy
    );
}

#[test]
fn recorded_copy_updates_in_copy_mode_after_target_switches_to_link() {
    let drive = Drive::new();
    fs::write(
        drive.registry.join("my-skills.local.toml"),
        "[targets.agy]\ninstall_mode = \"copy\"\n",
    )
    .unwrap();
    drive.sync(true);
    assert_eq!(
        link::entry_kind(&drive.host_dir("agy").join("alpha")).unwrap(),
        EntryKind::Dir
    );

    // Switch target mode to link and edit the canonical skill.
    fs::write(
        drive.registry.join("my-skills.local.toml"),
        "[targets.agy]\ninstall_mode = \"link\"\n",
    )
    .unwrap();
    let updated_skill = "---\nname: alpha\ndescription: Updated canonical.\n---\nUpdated body\n";
    fs::write(
        drive.registry.join("skills").join("alpha").join("SKILL.md"),
        updated_skill,
    )
    .unwrap();
    assert_eq!(drive.status("alpha", "agy"), Status::Stale);

    let items = drive.sync(true);
    let agy_alpha = items
        .iter()
        .find(|i| i.skill == "alpha" && i.host == "agy")
        .unwrap();
    assert_eq!(agy_alpha.action, Action::Update);
    assert_eq!(agy_alpha.mode, InstallMode::Copy);
    assert_eq!(
        link::entry_kind(&drive.host_dir("agy").join("alpha")).unwrap(),
        EntryKind::Dir
    );
    assert_eq!(
        fs::read_to_string(drive.host_dir("agy").join("alpha").join("SKILL.md")).unwrap(),
        updated_skill
    );
    assert_eq!(
        drive.state().get("alpha", "agy").unwrap().mode,
        InstallMode::Copy
    );
}
