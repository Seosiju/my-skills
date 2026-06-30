---
name: cli-inventory
description: Maintains a curated catalog of the agent-facing CLI services you actually use (gh→GitHub, twg→Atlassian, gcloud→Google Cloud, ...) and scans this machine to fill in which are installed, their version, and where they live — so you can identify and use these CLIs with few tokens and without errors. Also records the raw PATH/package-manager scan for full lookups. Use to learn what service CLIs are available before driving one.
---

# CLI Inventory

Keeps a **curated catalog of the agent CLI services you use** — the CLIs that
drive a service (GitHub, Jira, a cloud, an MCP/agent tool) — and scans this
machine to fill in the volatile parts: whether each is installed, its version,
where it resolves, and a coarse source label. The goal is a single compact table
you read once instead of re-probing each tool, so you identify and use these
CLIs with few tokens and without errors.

The canonical skill holds the catalog (`name` → `service`, both low-churn facts
you maintain by hand) and the scan *method*. Everything machine-specific — the
resolved paths, versions, the full PATH/package scan — is machine-local data at
the shared data root and is never committed.

## When to use

- When you need to know which agent CLI service is available for a task
  (e.g. "what do I call to work with GitHub / Jira / Google Cloud here?").
- Before driving a service CLI, to confirm it is installed and read its version
  and invocation path without probing the tool yourself.
- When you want the raw, full picture (every PATH executable, every package
  manager's packages) — that still lives in `inventory.json`.

## The catalog (what you maintain)

The catalog is the `CATALOG` list at the top of
[the scan script](scripts/scan_tools.py): a row per CLI service you use, each
just `(name, service)`. These are deliberately low-churn — a tool's provider
(`gh`→GitHub) almost never changes — so you set them once. The scan fills the
rest per machine into these columns:

| column | filled by | meaning |
| --- | --- | --- |
| `name` | you (catalog) | the command you invoke (`gh`, `twg`) |
| `service` | you (catalog) | the service it drives (GitHub, Atlassian) |
| `status` | scan | `available` if installed here, else `missing` |
| `version` | scan | the tool's reported version (probed safely) |
| `source` | scan | coarse origin: `brew` / `npm` / `local` / `PATH` / … |
| `path` | scan | where it resolves (the evidence the row is real) |

To add or drop a service, edit the `CATALOG` tuple. A name that is not installed
is reported as `missing` rather than dropped, so a gap is visible.

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

For a quick lookup, read `inventory.md` from the data root. It leads with the
**Agent CLI services** table (the catalog: name, service, status, version,
source, path), then an "expected tools" present/missing check and per-manager
package *counts*. The full package lists and the complete PATH executable map
(`path_tools`) stay in `inventory.json` so the readable summary stays cheap to
read.

If neither file exists yet, the machine has not been scanned — run the scan
below, then read the result.

## Refreshing the inventory (scan the machine)

Run [the scan script](scripts/scan_tools.py) to re-discover what is installed
and rewrite both files at the data root:

```bash
python3 scripts/scan_tools.py
```

It resolves every catalog entry (installed?, version, source, path), walks every
directory on `PATH` for executables, and queries each package manager that is
present, then writes `inventory.md` and `inventory.json` to the data root and
prints a short summary. It never installs anything — it only reads the machine
and records what it finds. To read a version it probes `--version`, then
`version`, then `-v`, taking the first real version line and rejecting
error/usage output. Re-run it whenever you install or remove tools, or edit the
catalog, and want the inventory current.

## Expected tools

Alongside the catalog, the script checks a small set of commonly needed tools
(for example `git`, `python3`, `uv`, `gh`, `rg`, `jq`) and reports any that are
missing, exposed as the `expected` block in both output files. This is separate
from the catalog: `expected` is a basic-deps present/missing check, while the
catalog is the curated service CLIs. To change either set, edit the `EXPECTED`
tuple or the `CATALOG` list at the top of `scripts/scan_tools.py`.

## Machine-local boundary

The canonical skill defines *how* to scan and *what* to highlight. Everything
machine-specific — the resolved tool paths, versions, hostnames, and the full
inventory — stays at the data root and is never committed. See
[the inventory schema](references/inventory-schema.md) for the output format and
this boundary in detail.
