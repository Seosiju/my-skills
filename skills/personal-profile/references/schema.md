# Personal profile schema

This describes the layout of the profile data stored at the profile root. The
profile root is the exact path printed by `my-skills data-path personal-profile`
(see the skill body for how to resolve it). Nothing here is personal data — it
is the format only.

## Layout

```text
<profile-root>/
├── profile.md          # stable core: identity + preferences
└── facts/              # one durable fact per file
    └── <slug>.md
```

- `profile.md` holds the small, slowly-changing core. Keep it short.
- `facts/` accumulates discrete durable facts, one per file, so entries can be
  added and revised independently. Name each file with a short kebab-case slug
  (for example `facts/preferred-editor.md`).

## profile.md format

Simple labelled lines; omit anything unknown rather than guessing.

```markdown
# Profile

- Name:
- Role:
- Timezone:
- Communication preferences:
- Tools / stack:
- Updated: 2026-06-27
```

## facts/<slug>.md format

```markdown
---
topic: preferred-editor
updated: 2026-06-27
---

The user prefers <editor> for <context>.
```

## What belongs here

- Identity: name, role, team, timezone, language.
- Preferences: communication style, formatting, tools and stack.
- Durable context: long-running projects, recurring constraints, standing
  instructions.

## What must NEVER be stored

- Secrets, passwords, API keys, access/auth tokens, session cookies.
- Anything the user marks as do-not-remember.

If a fact would require storing a secret, do not persist it.

## Rules

- Convert relative dates ("today", "last month") to absolute dates before
  saving, and stamp `Updated` / `updated`.
- Update an existing entry in place; do not create duplicates for the same fact.
- Prefer many small `facts/` files over one large blob — easier to revise and
  to remove a single fact later.
