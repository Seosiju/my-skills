# CLI inventory schema

This describes what `scripts/scan_tools.py` discovers and the files it writes to
the shared data root (see the skill body for how to resolve that root). Nothing
here is machine-specific — it is the format and the scan policy only.

## Output layout

```text
<data-root>/cli-inventory/
├── inventory.md     # readable summary: expected-tools check + manager packages
└── inventory.json   # complete record (every PATH executable + manager lists)
```

Both files are machine-local and are **never committed**.

## inventory.json fields

```json
{
  "generated": "<UTC ISO-8601 timestamp>",
  "host": "<hostname>",
  "platform": "<platform string>",
  "expected": { "<tool>": "<path or null if missing>" },
  "path_tools": { "<executable name>": "<resolved path>" },
  "managers": { "<manager>": ["<package line>", "..."] }
}
```

- `expected` — the small set of commonly needed tools, each resolved to its path
  or `null`. Driven by the `EXPECTED` tuple in `scripts/scan_tools.py`.
- `path_tools` — every executable found on `PATH`, name → path, first PATH
  directory winning on collisions (so the path reflects what actually runs).
- `managers` — for each package manager present on `PATH`, the raw list of
  packages it reports. Absent managers are omitted.

## What is scanned

1. **PATH** — every directory on `PATH` is walked for executable files.
2. **Package managers** — each of `brew`, `npm` (global), `pipx`, `cargo`,
   `gem`, and `pip` is queried only if it resolves on `PATH`; missing managers
   are skipped silently.

To extend the scan, edit the `EXPECTED` set or the `MANAGERS` map at the top of
`scripts/scan_tools.py`.

## Machine-local data boundary

The scan policy above is the only thing committed. Everything the scan produces
is machine-specific and stays out of version control:

- resolved executable paths and versions for this machine
- hostname and platform
- which package managers exist and what they installed

Those live only at the data root (`<data-root>/cli-inventory/`), which is
machine-local and never committed — the same root `personal-profile` uses. The
canonical skill defines *what* to scan and *how*; the data root holds *the
answers for this computer*.
