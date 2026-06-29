---
name: cli-inventory
description: Discovers the command-line tools installed on this machine — scanning PATH and package managers (Homebrew, npm, pipx, cargo, gem, pip) — and records them to a shared inventory you can read back quickly. Use when you need to know whether a tool is installed, find where it lives, or get an overview of the available CLIs before running a workflow.
---

# CLI Inventory

Finds the command-line tools installed on the current machine and keeps a
readable inventory of them. The canonical skill holds only the scan *method*;
the actual results are machine-local data stored at the shared data root and are
never committed.

## When to use

- Before a workflow that shells out to external tools, to confirm what is
  installed and where.
- When you need to know whether a specific CLI exists on this machine, or find
  its path or version.
- When you want an overview of the available CLIs (PATH executables and
  packages installed via Homebrew, npm, pipx, cargo, gem, or pip).

## Where the inventory lives

The inventory is stored at the **shared data root**, not inside this skill
directory. It is the same machine-local location every host shares, so Claude,
Codex, and Hermes all read the one inventory:

1. If the `my-skills` CLI is available, run `my-skills data-path cli-inventory`
   (add `--create` to make it on first use). Use the path it prints.
2. Otherwise follow the rule directly: `$XDG_DATA_HOME/my-skills/cli-inventory`
   if `XDG_DATA_HOME` is set, else `~/.local/share/my-skills/cli-inventory`.

Two files live there: `inventory.md` (a readable summary) and `inventory.json`
(the complete record). Both are machine-local and never committed.

## Reading the inventory (do this first)

For a quick lookup, read `inventory.md` from the data root. It lists the
"expected tools" present/missing check and every package installed via a package
manager. For the full set of PATH executables (names → paths), read the
`path_tools` map in `inventory.json`.

If neither file exists yet, the machine has not been scanned — run the scan
below, then read the result.

## Refreshing the inventory (scan the machine)

Run [the scan script](scripts/scan_tools.py) to re-discover what is installed
and rewrite both files at the data root:

```bash
python3 scripts/scan_tools.py
```

It walks every directory on `PATH` for executables and queries each package
manager that is present, then writes `inventory.md` and `inventory.json` to the
data root and prints a short summary. It never installs anything — it only reads
the machine and records what it finds. Re-run it whenever you install or remove
tools and want the inventory current.

## Expected tools

The script also checks a small set of commonly needed tools (for example `git`,
`python3`, `uv`, `gh`, `rg`, `jq`) and reports any that are missing, exposed as
the `expected` block in both output files. To change that set, edit the
`EXPECTED` tuple at the top of `scripts/scan_tools.py`.

## Machine-local boundary

The canonical skill defines *how* to scan and *what* to highlight. Everything
machine-specific — the resolved tool paths, versions, hostnames, and the full
inventory — stays at the data root and is never committed. See
[the inventory schema](references/inventory-schema.md) for the output format and
this boundary in detail.
