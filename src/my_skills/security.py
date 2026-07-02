"""Static security scan for canonical skills (plan section 15).

A skill is executable instruction, not inert documentation, so it is scanned
before it is trusted. Findings are reported, never auto-fixed.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

# Bidirectional / hidden Unicode control characters (Trojan Source class):
# LRE RLE PDF LRO RLO, LRI RLI FSI PDI, LRM RLM. Written as code points so the
# source file itself stays free of the very characters it detects.
_BIDI_CHARS = frozenset(
    chr(cp)
    for cp in (
        0x202A, 0x202B, 0x202C, 0x202D, 0x202E,
        0x2066, 0x2067, 0x2068, 0x2069,
        0x200E, 0x200F,
    )
)

_PRIVATE_KEY_RE = re.compile(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----")
_AWS_KEY_RE = re.compile(r"\bAKIA[0-9A-Z]{16}\b")
_SECRET_ASSIGN_RE = re.compile(
    r"(?i)\b(?:api[_-]?key|secret|access[_-]?token|auth[_-]?token|password|passwd)\b"
    r"\s*[:=]\s*['\"]?[A-Za-z0-9_\-/+]{8,}"
)
_ABS_USER_PATH_RE = re.compile(r"/Users/[^/\s]+|/home/[^/\s]+")

# Suffixes treated as scannable text. Other files are only checked for NUL bytes.
TEXT_SUFFIXES = {
    ".md",
    ".txt",
    ".py",
    ".sh",
    ".yaml",
    ".yml",
    ".json",
    ".toml",
    ".cfg",
    ".ini",
    "",
}


@dataclass
class Finding:
    file: str
    rule: str
    message: str
    severity: str = "error"  # "error" | "warning"


def scan_text(rel: str, text: str) -> list[Finding]:
    """Scan already-decoded *text* for the security rules. Pure / testable."""
    findings: list[Finding] = []
    if any(ch in _BIDI_CHARS for ch in text):
        findings.append(
            Finding(rel, "bidi-unicode", "hidden/bidirectional Unicode control character")
        )
    if _PRIVATE_KEY_RE.search(text):
        findings.append(Finding(rel, "private-key", "embedded private key header"))
    if _AWS_KEY_RE.search(text):
        findings.append(Finding(rel, "aws-key", "AWS access key id pattern"))
    if _SECRET_ASSIGN_RE.search(text):
        findings.append(Finding(rel, "secret", "secret-like assignment"))
    if _ABS_USER_PATH_RE.search(text):
        findings.append(
            Finding(
                rel,
                "abs-user-path",
                "absolute user/home path leak",
                severity="warning",
            )
        )
    return findings


def scan_skill(path: Path) -> list[Finding]:
    """Walk a skill directory and return all security findings."""
    path = Path(path)
    findings: list[Finding] = []
    for file in sorted(path.rglob("*")):
        if not file.is_file():
            continue
        rel = str(file.relative_to(path))
        try:
            raw = file.read_bytes()
        except OSError:
            continue
        if b"\x00" in raw:
            findings.append(Finding(rel, "nul-byte", "file contains a NUL byte"))
        if file.suffix.lower() not in TEXT_SUFFIXES:
            continue
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            findings.append(Finding(rel, "encoding", "file is not valid UTF-8"))
            continue
        findings.extend(scan_text(rel, text))
    return findings
