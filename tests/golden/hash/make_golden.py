"""Regenerate the directory hashes using the v0.4.1 Python implementation."""

import base64
import json
from pathlib import Path
from tempfile import TemporaryDirectory

from my_skills import __version__
from my_skills.hashing import hash_directory


def main() -> None:
    if __version__ != "0.4.1":
        raise SystemExit(f"Golden data requires my-skills 0.4.1, got {__version__}")

    fixtures = Path(__file__).resolve().parent
    cases = json.loads((fixtures / "cases.json").read_text(encoding="utf-8"))
    hashes = {}
    with TemporaryDirectory() as temp_dir:
        for case in cases:
            root = Path(temp_dir) / case["name"]
            root.mkdir()
            for directory in case["dirs"]:
                (root / directory).mkdir(parents=True, exist_ok=True)
            for relative_path, encoded in case["files"].items():
                file = root / relative_path
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes(base64.b64decode(encoded, validate=True))
            hashes[case["name"]] = hash_directory(root)

    golden = json.dumps(hashes, indent=2, sort_keys=True) + "\n"
    (fixtures / "golden.json").write_text(golden, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
