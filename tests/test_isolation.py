from __future__ import annotations

from my_skills.cli_runtime import _root_cache_path
from my_skills.data import data_root
from my_skills.state import default_state_path


def test_default_user_paths_are_isolated_under_pytest_tmp(tmp_path):
    assert tmp_path in default_state_path().parents
    assert tmp_path in _root_cache_path().parents
    assert tmp_path in data_root().parents
