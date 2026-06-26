---
name: shared-agent-operation
description: Baseline operating conventions shared across AI coding agents. Use when an agent needs consistent guidance for reading a repository, making minimal changes, verifying outcomes, and reporting results regardless of which host it runs in.
---

# Shared Agent Operation

Host-neutral conventions any agent should follow when working in a repository.

## When to use

Apply this skill at the start of a task to align on shared working
conventions, or when a task spans multiple tools and you want consistent
behavior across them.

## Conventions

1. Understand before changing. Read the relevant files and existing patterns
   before editing.
2. Prefer the smallest change that solves the problem.
3. Match the surrounding code's style and naming.
4. Verify outcomes with evidence (run the relevant checks) before claiming a
   task is complete.
5. Report what changed, what was verified, and anything left open.

## Reading order

Start from the entry point relevant to the task, follow references outward, and
read only what the task requires.

## Notes

Keep instructions portable. Refer to files by their path inside the skill
directory (for example, a `references` folder) rather than any host's absolute
install location.
