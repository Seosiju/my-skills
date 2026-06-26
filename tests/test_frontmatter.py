import pytest

from my_skills.frontmatter import FrontmatterError, parse_frontmatter


def test_valid_frontmatter_returns_meta_and_body():
    text = "---\nname: x\ndescription: y\n---\n\n# Body\nhello\n"
    meta, body = parse_frontmatter(text)
    assert meta == {"name": "x", "description": "y"}
    assert "# Body" in body
    assert "hello" in body


def test_missing_opening_delimiter():
    with pytest.raises(FrontmatterError, match="missing opening"):
        parse_frontmatter("# No frontmatter\nbody\n")


def test_unterminated_frontmatter():
    with pytest.raises(FrontmatterError, match="unterminated"):
        parse_frontmatter("---\nname: x\ndescription: y\n\n# body\n")


def test_malformed_yaml_wrapped():
    with pytest.raises(FrontmatterError, match="malformed YAML"):
        parse_frontmatter('---\nname: "oops\n---\n# body\n')


def test_empty_text():
    with pytest.raises(FrontmatterError):
        parse_frontmatter("")


def test_non_mapping_frontmatter():
    with pytest.raises(FrontmatterError, match="mapping"):
        parse_frontmatter("---\n- a\n- b\n---\nbody\n")


def test_bom_tolerated():
    meta, _ = parse_frontmatter("﻿---\nname: x\ndescription: y\n---\nbody\n")
    assert meta["name"] == "x"
