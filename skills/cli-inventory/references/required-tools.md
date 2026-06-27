# Required-tool policy

This is the canonical, machine-independent policy for `cli-inventory`: the
command-line tools a workflow in this registry may depend on, and why each is
needed. It is safe to commit because it names tools and reasons only — never a
specific machine's state.

## Required tools

| Tool | Purpose | Required |
|------|---------|----------|
| `git` | Version control: clone, status, diff, commit. | yes |
| `python3` | Run skill scripts and the `my-skills` CLI (Python 3.11+). | yes |
| `uv` | Create the environment and run the project (`uv run ...`). | yes |

## Optional tools

| Tool | Purpose |
|------|---------|
| `gh` | GitHub operations (PRs, releases) when a workflow needs them. |
| `rg` | Faster code search than `grep` when available. |

## How availability is checked

`scripts/check_tools.py` resolves each required tool on the current `PATH`
(`shutil.which`) and reports which are present and which are missing. It exits
non-zero if any **required** tool is missing. It never installs anything — the
report is for a human or agent to act on.

To extend the policy, edit the `REQUIRED` / `OPTIONAL` lists in
`scripts/check_tools.py` and the tables above together.

## Machine-local data boundary

The policy above is the only thing committed. Everything machine-specific stays
out of the canonical skill and out of version control:

- actual `PATH` resolution results for this machine
- absolute install paths of the tools
- versions, account names, and auth/login state
- hostnames or any other host identity

Those belong in a machine-local, git-ignored location:

```text
local/cli-inventory/          # git-ignored; never committed
```

The repository's `.gitignore` excludes `local/`, so anything written there is
private to the machine. The canonical skill defines *what* to check and *why*;
the machine-local directory holds *the answers for this computer*.
