---
name: personal-profile
description: Remembers durable facts about the user (identity, preferences, recurring context) and applies them across agents. Use when a task would benefit from knowing who the user is or how they like to work, or when the user shares a lasting personal fact worth remembering for next time.
---

# Personal Profile

A memory-like skill. It keeps a small, durable profile of the user — who they
are and how they prefer to work — so any agent can read it before acting and add
to it when the user shares something lasting. The canonical skill holds only the
instructions and schema; the actual profile is machine-local data that every
host shares and that is never committed.

## When to use

- Before a task where the user's identity, role, timezone, or working
  preferences would change the answer.
- When the user states a durable fact about themselves ("I prefer X", "my role
  is Y", "always do Z for me") that should persist to the next session.

Do not use it for one-off, task-local details that won't matter later.

## Where the profile lives

The profile is stored at the **shared data root**, not inside this skill
directory. It is the same location for every host, so Claude, Codex, and Hermes
all read and write the one profile.

Resolve the directory host-neutrally:

1. If the `my-skills` CLI is available, run `my-skills data-path personal-profile`
   (add `--create` to make it on first use). Use the path it prints.
2. Otherwise follow the rule directly: `$XDG_DATA_HOME/my-skills/personal-profile`
   if `XDG_DATA_HOME` is set, else `~/.local/share/my-skills/personal-profile`.

Never hardcode a host's skill-install path; always resolve the data root this way.

## Reading (do this first)

At the start of a relevant task, read the profile from the data root before
answering. If the directory or files do not exist yet, treat the profile as
empty — do not invent facts. Apply what you find; do not restate it back to the
user unless asked.

## Writing (persist durable facts)

When the user shares a lasting fact:

1. Resolve the data root (create it if missing).
2. Save the fact following [the profile schema](references/schema.md): core
   identity/preferences go in `profile.md`; a distinct durable fact goes in its
   own file under `facts/`.
3. Convert any relative date ("today", "last week") to an absolute date before
   saving.
4. Update an existing entry in place instead of adding a duplicate.

## Privacy rules

- The data root is machine-local and is **never committed** to the repository.
- The canonical skill (this directory) must contain **no personal data**.
- **Never** store secrets, passwords, API keys, tokens, or auth state here. If
  the user offers one, decline to persist it and explain why.
