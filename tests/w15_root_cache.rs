//! Wave 15: an explicit set-root reports a root cache it could not save.

mod support_h;

use std::fs;

use predicates::prelude::*;
use predicates::str::contains;
use support_h::Fixture;

#[test]
fn set_root_fails_when_the_root_cache_lock_cannot_be_opened() {
    let fixture = Fixture::repo();
    fixture.write_manifest("");
    let other = fixture.root.join("registry-b");
    fs::create_dir_all(other.join("skills")).unwrap();
    fs::copy(fixture.manifest(), other.join("my-skills.toml")).unwrap();

    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["set-root"])
        .arg(&fixture.registry)
        .assert()
        .success();
    let cache = fixture.root.join("config").join("my-skills").join("root");
    let before = fs::read(&cache).unwrap();

    // A directory where the lock file belongs makes the lock open fail.
    let lock = cache.with_file_name("root.lock");
    let _ = fs::remove_file(&lock);
    fs::create_dir_all(&lock).unwrap();
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["set-root"])
        .arg(&other)
        .assert()
        .code(2)
        .stdout(contains("Active registry root set to").not())
        .stderr(contains("error:"));
    assert_eq!(fs::read(&cache).unwrap(), before);
}
