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
