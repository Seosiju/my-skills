from pathlib import Path

from my_skills.security import scan_skill, scan_text

FIXTURES = Path(__file__).parent / "fixtures"


def test_clean_skill_has_no_findings():
    assert scan_skill(FIXTURES / "good-skill") == []


def test_secret_skill_flagged():
    rules = {f.rule for f in scan_skill(FIXTURES / "secret-skill")}
    assert {"secret", "aws-key"} & rules


def test_private_key_detected():
    findings = scan_text("k.txt", "-----BEGIN RSA PRIVATE KEY-----\nabc\n")
    assert any(f.rule == "private-key" for f in findings)


def test_bidi_unicode_detected():
    findings = scan_text("b.md", "hello" + chr(0x202E) + "world")
    assert any(f.rule == "bidi-unicode" for f in findings)


def test_abs_user_path_is_warning_severity():
    findings = scan_text("p.md", "see /Users/alice/notes for details")
    abs_findings = [f for f in findings if f.rule == "abs-user-path"]
    assert abs_findings and abs_findings[0].severity == "warning"


def test_generic_host_default_paths_are_not_user_path_leaks():
    findings = scan_text(
        "p.md",
        "Host directories: ~/.claude/skills, ~/.agents/skills, ~/.hermes/skills.",
    )

    assert not [f for f in findings if f.rule == "abs-user-path"]


def test_nul_byte_detected(tmp_path):
    d = tmp_path / "nul-skill"
    d.mkdir()
    (d / "SKILL.md").write_bytes(
        b"---\nname: nul-skill\ndescription: x\n---\nbody\x00here\n"
    )
    assert any(f.rule == "nul-byte" for f in scan_skill(d))


def test_scan_skill_ignores_runtime_artifacts_with_nul_bytes(tmp_path):
    # Given: a clean skill plus runtime/system artifacts that can contain binary data.
    d = tmp_path / "runtime-artifacts"
    d.mkdir()
    (d / "SKILL.md").write_text(
        "---\nname: runtime-artifacts\ndescription: Valid skill.\n---\n\n# Body\n"
    )
    (d / "__pycache__").mkdir()
    (d / "__pycache__" / "x.pyc").write_bytes(b"cache\x00data")
    (d / ".omc").mkdir()
    (d / ".omc" / "state.json").write_bytes(b'{"state":"ok"}\x00')
    (d / ".DS_Store").write_bytes(b"finder\x00state")

    # When: the security scanner walks the skill directory.
    findings = scan_skill(d)

    # Then: ignored runtime/system artifacts do not produce findings.
    assert findings == []


def test_scan_skill_still_detects_nul_bytes_in_regular_files(tmp_path):
    # Given: a normal skill script containing a NUL byte.
    d = tmp_path / "regular-nul"
    (d / "scripts").mkdir(parents=True)
    (d / "SKILL.md").write_text(
        "---\nname: regular-nul\ndescription: Valid skill.\n---\n\n# Body\n"
    )
    (d / "scripts" / "nul.py").write_bytes(b"print('hi')\x00\n")

    # When: the security scanner walks the skill directory.
    findings = scan_skill(d)

    # Then: regular content is still scanned.
    assert [(f.file, f.rule) for f in findings] == [("scripts/nul.py", "nul-byte")]
