---
name: repo-analysis
description: Surveys an unfamiliar code repository and produces a structured summary of its purpose, layout, build/test commands, and entry points. Use when you join a new repo, need to orient before making a change, or are asked "what does this project do / how is it organized / how do I build and test it".
---

# Repo Analysis

A host-neutral routine for orienting yourself in a repository before changing it.
It produces a short, evidence-backed map of the project rather than guessing.

## When to use

Use this skill at the start of work in an unfamiliar repository, or when someone
asks what a project does, how it is structured, or how to build and test it.

## Procedure

1. **Identify the project type.** Look for manifest/build files at the root
   (for example `pyproject.toml`, `package.json`, `Cargo.toml`, `go.mod`,
   `pom.xml`, `Makefile`). They reveal the language, dependencies, and the
   declared build/test commands.
2. **Map the top-level layout.** List the root directories and note the source
   root, tests, docs, and configuration. Do not read every file — read names
   first, then open only what the question requires.
3. **Find the entry points.** Locate `main`, CLI definitions, server bootstrap,
   or the package's public exports. Start reading from the entry point relevant
   to the task and follow references outward.
4. **Recover the build/test/run commands.** Prefer commands declared in the
   manifest or CI config over guessing. Record the exact commands.
5. **Read the project's own docs.** Check `README`, `CONTRIBUTING`, and any
   `docs/` for stated conventions before inferring your own.
6. **Summarize.** Produce the report described in
   [the analysis checklist](references/analysis-checklist.md).

## Output

A concise summary covering: project purpose, language/runtime, top-level layout,
how to build, how to test, how to run, key entry points, and any open questions.
Cite the file each fact came from so the summary is verifiable.

## Notes

Keep the procedure portable. Refer to files by their path inside the repository
or inside this skill directory (for example, the `references` folder) rather than
any host's absolute install location.
