# Repo analysis report checklist

Fill each field from evidence in the repository. Cite the file (and line where
useful) that each fact comes from. Leave a field blank and list it under "Open
questions" rather than guessing.

## Report fields

- **Purpose** — one or two sentences on what the project is for.
- **Language / runtime** — primary language(s) and the required runtime version.
- **Top-level layout** — the root directories and what each holds (source,
  tests, docs, config).
- **Entry points** — where execution begins (CLI, server bootstrap, public
  package exports).
- **Build** — the exact command(s) to build or install dependencies.
- **Test** — the exact command(s) to run the test suite.
- **Run** — the exact command(s) to run the project locally, if applicable.
- **Conventions** — notable rules from `README` / `CONTRIBUTING` / `docs/`.
- **Open questions** — anything the repository did not make clear.

## Evidence sources, in order of trust

1. Manifest / build files (declared dependencies and scripts).
2. CI configuration (the commands the project actually runs).
3. Project documentation (`README`, `CONTRIBUTING`, `docs/`).
4. The code itself (entry points and references).

Prefer a declared command over an inferred one. If two sources disagree, report
both and flag the discrepancy.
