# CLI inventory schema

This describes what `scripts/scan_tools.py` discovers and the files it writes to
the shared data root (see the skill body for how to resolve that root). Nothing
here is machine-specific — it is the format and the scan policy only.

## Output layout

```text
<data-root>/cli-inventory/
├── inventory.md     # readable summary: catalog table + expected check + counts
└── inventory.json   # complete record (catalog + every PATH executable + lists)
```

Both files are machine-local and are **never committed**.

## inventory.json fields

```json
{
  "generated": "<UTC ISO-8601 timestamp>",
  "host": "<hostname>",
  "platform": "<platform string>",
  "catalog": [
    {
      "name": "<command>",
      "service": "<service it drives>",
      "status": "available | missing",
      "path": "<resolved path or null>",
      "version": "<reported version line or null>",
      "source": "<brew | npm | local | cargo | conda/pip | PATH, or null>"
    }
  ],
  "expected": { "<tool>": "<path or null if missing>" },
  "path_tools": { "<executable name>": "<resolved path>" },
  "managers": { "<manager>": ["<package line>", "..."] }
}
```

- `catalog` — the curated agent CLI-service catalog. `name` and `service` are
  authored in the `CATALOG` list in `scripts/scan_tools.py` (low-churn facts);
  `status`, `path`, `version`, and `source` are resolved per machine by the
  scan. A catalog name that is not installed appears with `status: "missing"`
  and null path/version/source — it is reported, not dropped.
- `expected` — the small set of commonly needed tools, each resolved to its path
  or `null`. Driven by the `EXPECTED` tuple in `scripts/scan_tools.py`.
- `path_tools` — every executable found on `PATH`, name → path, first PATH
  directory winning on collisions (so the path reflects what actually runs).
- `managers` — for each package manager present on `PATH`, the raw list of
  packages it reports. Absent managers are omitted.

## What is scanned

1. **Catalog** — each `(name, service)` in `CATALOG` is resolved against this
   machine: installed path (via PATH then `which`), version, and source.
2. **PATH** — every directory on `PATH` is walked for executable files.
3. **Package managers** — each of `brew`, `npm` (global), `pipx`, `cargo`,
   `gem`, and `pip` is queried only if it resolves on `PATH`; missing managers
   are skipped silently.

Version detection probes `--version`, then `version`, then `-v`, takes the first
real version line (stdout preferred), and rejects error/usage messages so an
unsupported flag is not recorded as a version. Each probe is bounded by a short
timeout.

To extend the scan, edit the `CATALOG` list, the `EXPECTED` set, or the
`MANAGERS` map at the top of `scripts/scan_tools.py`.

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
