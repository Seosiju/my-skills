---
name: cli-inventory
description: Scans the CLI commands you deliberately installed on this machine — your service CLIs (gh→GitHub, twg→Atlassian, gcloud→Google Cloud, ...), the individual tools in your user bin dirs, and your top-level Homebrew formulae — with each one's version, source, and path, recorded to a shared inventory you read back quickly. Skips the thousands of library/script executables so the summary stays small. Use to learn which command drives a service, confirm a tool is installed, or get a compact picture of your toolkit before a workflow.
---

# CLI Inventory

Lists the CLI commands you **deliberately installed** on this machine — not the
thousands of library shims and system scripts on `PATH`, just the things you
actually invoke — each with its version, source, and path. The result is a few
dozen rows you read once instead of re-probing tools, so you can find which
command drives a service and use it without errors, cheaply.

The output is grouped so signal stays on top:

1. **Service CLIs** — the labelled CLIs that drive a service (GitHub, Jira, a
   cloud, an MCP/agent tool), resolved live by command name.
2. **Other user-installed commands** — individual binaries in your user bin
   directories (`~/.local/bin`, `~/bin`, `~/.cargo/bin`, the active node bin).
3. **Homebrew tools** — your top-level (leaf) formulae, one row each.

A newly installed command lands in one of these places, so it shows up on the
next scan automatically — there is no hand-curated whitelist that hides what you
forgot to add. The canonical skill holds only the scan *method* and two small
edit points (`SERVICE_LABELS`, `EXCLUDE`); everything machine-specific — the
resolved paths, versions, the full raw scan — is machine-local data at the
shared data root and is never committed.

## When to use

- When you need to know which command drives a service here ("what do I call to
  work with GitHub / Jira / Google Cloud?").
- Before driving a tool, to confirm it is installed and read its version and
  path without probing the tool yourself.
- When you want a compact picture of your deliberately-installed toolkit. (The
  raw, full scan — every PATH executable, every package — still lives in
  `inventory.json`.)

## What is listed (and the two edit points)

The scan derives the list from where deliberate installs land, so you do not
maintain a catalog of what exists. Two optional knobs at the top of
[the scan script](scripts/scan_tools.py) shape the output:

- **`SERVICE_LABELS`** — `command → service` annotations (`gh`→GitHub,
  `twg`→Atlassian). A labelled command that is installed is lifted into the
  **Service CLIs** table; everything else still appears below, so a missing
  label never hides a tool. Add a line to call a command out.
- **`EXCLUDE`** — glob patterns for internal noise to drop entirely
  (default `omo-*`, oh-my-claudecode's hook scripts).

Columns: `name` (the command, or formula name for Homebrew leaves), `service`
(Service CLIs only), `version`, `source` (`brew` / `npm` / `local` / `cargo` /
`PATH` / …), and `path`. Homebrew leaves are listed one row per *formula*, not
per shipped binary, so a multi-binary formula like `coreutils` counts once.

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
three command groups (Service CLIs, other user commands, Homebrew leaves), then
an "expected tools" present/missing check and per-manager package *counts*. The
full package lists and the complete PATH executable map (`path_tools`) stay in
`inventory.json` so the readable summary stays cheap to read.

If neither file exists yet, the machine has not been scanned — run the scan
below, then read the result.

## Refreshing the inventory (scan the machine)

Run [the scan script](scripts/scan_tools.py) to re-discover what is installed
and rewrite both files at the data root:

```bash
python3 scripts/scan_tools.py
```

It resolves the labelled service CLIs, lists the binaries in your user bin
directories and your Homebrew leaf formulae, also captures the full raw PATH and
package-manager scan (for `inventory.json`), then writes `inventory.md` and
`inventory.json` and prints a short summary. It never installs anything — it only
reads the machine. To read a version it probes `--version`, then `version`, then
`-v`, takes the first real version line, and rejects error/usage output; closing
stdin so a tool cannot hang the scan. Homebrew leaf versions come from
`brew list --versions`, so those are not probed. Re-run it whenever you install
or remove tools and want the inventory current.

## Expected tools

Separately, the script checks a small set of commonly needed tools (for example
`git`, `python3`, `uv`, `gh`, `rg`, `jq`) and reports any that are missing,
exposed as the `expected` block in both output files. To change that set, edit
the `EXPECTED` tuple at the top of `scripts/scan_tools.py`.

## Machine-local boundary

The canonical skill defines *how* to scan and *what* to highlight. Everything
machine-specific — the resolved tool paths, versions, hostnames, and the full
inventory — stays at the data root and is never committed. So the same skill on
a different machine produces a different inventory: the same scan method, that
computer's answers. See [the inventory schema](references/inventory-schema.md)
for the output format and this boundary in detail.
