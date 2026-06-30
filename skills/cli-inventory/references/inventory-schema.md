# CLI inventory schema

This describes what `scripts/scan_tools.py` discovers and the files it writes to
the shared data root (see the skill body for how to resolve that root). Nothing
here is machine-specific — it is the format and the scan policy only.

## Output layout

```text
<data-root>/cli-inventory/
├── inventory.md     # readable summary: 3 command groups + expected + counts
└── inventory.json   # complete record (groups + full raw PATH/package scan)
```

Both files are machine-local and are **never committed**.

## inventory.json fields

```json
{
  "generated": "<UTC ISO-8601 timestamp>",
  "host": "<hostname>",
  "platform": "<platform string>",
  "service_clis": [
    {
      "name": "<command>",
      "service": "<service it drives>",
      "version": "<reported version line or null>",
      "source": "<brew | npm | local | cargo | conda/pip | PATH>",
      "path": "<resolved path>"
    }
  ],
  "user_commands": [
    {
      "name": "<command>",
      "version": "<reported version line or null>",
      "source": "<source label>",
      "path": "<resolved path>"
    }
  ],
  "brew_leaves": [
    { "name": "<formula>", "version": "<version or null>" }
  ],
  "expected": { "<tool>": "<path or null if missing>" },
  "path_tools": { "<executable name>": "<resolved path>" },
  "managers": { "<manager>": ["<package line>", "..."] }
}
```

- `service_clis` — labelled CLIs (from `SERVICE_LABELS`) that resolve on this
  machine, by command name. A label that is not installed is omitted.
- `user_commands` — individual executables in the user bin directories
  (`~/.local/bin`, `~/bin`, `~/.cargo/bin`, the active node bin), excluding the
  labelled services above and any `EXCLUDE` glob match.
- `brew_leaves` — top-level (leaf) Homebrew formulae, one entry per formula
  (not per shipped binary); versions read from `brew list --versions`.
- `expected` — the small set of commonly needed tools, each resolved to its path
  or `null`. Driven by the `EXPECTED` tuple in `scripts/scan_tools.py`.
- `path_tools` — every executable found on `PATH`, name → path, first PATH
  directory winning on collisions (so the path reflects what actually runs).
- `managers` — for each package manager present on `PATH`, the raw list of
  packages it reports. Absent managers are omitted.

## What is scanned

1. **Service CLIs** — each name in `SERVICE_LABELS` is resolved via `which`;
   installed ones get version + source + path.
2. **User commands** — executables in the user bin directories, minus
   `SERVICE_LABELS` and `EXCLUDE`.
3. **Homebrew leaves** — `brew leaves` (top-level formulae), versioned from
   `brew list --versions`.
4. **PATH** — every directory on `PATH` is walked for executable files (raw).
5. **Package managers** — each of `brew`, `npm` (global), `pipx`, `cargo`,
   `gem`, and `pip` is queried only if it resolves on `PATH`; missing managers
   are skipped silently (raw).

Version detection probes `--version`, then `version`, then `-v`, takes the first
real version line (stdout preferred), and rejects error/usage messages so an
unsupported flag is not recorded as a version. Each probe closes stdin and is
bounded by a short timeout; Homebrew leaf versions are read from the manager
output rather than probed.

To shape the output, edit `SERVICE_LABELS` (labels), `EXCLUDE` (noise globs), or
`EXPECTED` (basics check) at the top of `scripts/scan_tools.py`.

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
