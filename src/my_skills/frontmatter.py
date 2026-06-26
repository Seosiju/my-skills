"""Parse the YAML frontmatter block of a ``SKILL.md`` file.

The Agent Skills standard stores skill metadata in a leading ``---`` delimited
YAML block. This module isolates that parsing so failures surface as precise
:class:`FrontmatterError` messages rather than raw YAML tracebacks.
"""

from __future__ import annotations

import yaml

_DELIM = "---"
_BOM = chr(0xFEFF)


class FrontmatterError(ValueError):
    """Raised when a frontmatter block is missing or malformed."""


def parse_frontmatter(text: str) -> tuple[dict, str]:
    """Split a leading ``---`` delimited YAML frontmatter block from the body.

    Returns ``(metadata, body)``. Raises :class:`FrontmatterError` with a
    precise message on any structural or YAML problem.
    """
    if not text:
        raise FrontmatterError("file is empty: no frontmatter found")

    # Tolerate a leading UTF-8 BOM.
    if text.startswith(_BOM):
        text = text[len(_BOM) :]

    lines = text.splitlines()

    if not lines or lines[0].strip() != _DELIM:
        raise FrontmatterError(
            "missing opening '---': frontmatter must start on the first line"
        )

    closing = None
    for i in range(1, len(lines)):
        if lines[i].strip() == _DELIM:
            closing = i
            break

    if closing is None:
        raise FrontmatterError(
            "unterminated frontmatter: no closing '---' delimiter found"
        )

    fm_text = "\n".join(lines[1:closing])
    body = "\n".join(lines[closing + 1 :])

    try:
        data = yaml.safe_load(fm_text)
    except yaml.YAMLError as exc:
        raise FrontmatterError(f"malformed YAML in frontmatter: {exc}") from exc

    if data is None:
        data = {}
    if not isinstance(data, dict):
        raise FrontmatterError(
            f"frontmatter must be a YAML mapping, got {type(data).__name__}"
        )

    return data, body
