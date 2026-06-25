---
name: cli-inventory
description: Declares the command-line tools a workflow requires and checks whether they are available. Use when a task depends on external CLIs and you need to confirm the environment provides them before proceeding.
---

# CLI Inventory

Declares the command-line tools a workflow depends on and verifies their
presence, without storing any machine-specific data in the canonical skill.

## When to use

Use this skill before running a workflow that shells out to external tools, to
confirm the required commands exist and to report any that are missing.

## Policy

- The canonical skill records only the required-tool policy: tool names and why
  they are needed.
- Actual hostnames, absolute paths, account names, and auth state are
  machine-local data and are never committed to the canonical skill.

## Required tools (example policy)

- A version-control client for repository operations.
- A package or environment manager for dependency setup.

## Checking availability

For each required tool, check whether it resolves on the current PATH and
report the missing ones. Do not attempt to install tools automatically.
