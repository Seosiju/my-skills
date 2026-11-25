# my-skills

**Keep your agent skills in one place. Use them in every coding agent.**

**English** | [한국어](README.ko.md)

[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![spec: Agent Skills](https://img.shields.io/badge/spec-Agent%20Skills-green.svg)](https://agentskills.io/specification)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)

`my-skills` is a small CLI for people who write their own
[Agent Skills](https://agentskills.io/specification). You keep your skills in one
folder, your *registry*, which is a Git repo. my-skills connects that folder to
Claude Code, Codex and other agents, so you edit a skill once and every agent
sees the change.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/how-it-works-dark.png">
  <img alt="How my-skills works: you edit a skill in your registry, a Git repo. The my-skills CLI links it into Claude Code, Codex and Antigravity CLI. You push the registry to a private remote, and on another machine git pull runs a hook that updates that machine's agents. A copy edited by hand is reported as drifted and is not overwritten." src="docs/images/how-it-works.png">
</picture>

- **One place for your skills.** Each skill is a folder with a `SKILL.md`.
- **Edit once, every agent sees it.** Agents get a link to the original, not a copy.
- **The same skills on every machine.** Push the registry to a private Git repo,
  and `git pull` on another machine updates its agents.

## Install the CLI

macOS or Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/Seosiju/my-skills/main/scripts/install.sh -o my-skills-install.sh
sh ./my-skills-install.sh
```

Windows (PowerShell):

```powershell
Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/Seosiju/my-skills/main/scripts/install.ps1' -OutFile '.\my-skills-install.ps1'
.\my-skills-install.ps1 -AddToPath
```

The installers download the latest release, check its SHA-256, and install it to
`~/.local/bin` (Windows: `%LOCALAPPDATA%\Programs\my-skills\bin`). Open a new
terminal afterwards and run `my-skills --version`. To update later, run
`my-skills update`.

## Get started

Create a registry. my-skills asks where to put it (`~/my-agent-skills` by
default) and sets it up as a Git repo with a few starter skills.

```bash
my-skills init-registry
my-skills install --dry-run   # see what will be connected
my-skills install             # connect the skills to your agents
```

Already have a registry on another machine? Clone it and connect it:

```bash
git clone <your-private-repo> ~/my-agent-skills
cd ~/my-agent-skills
my-skills bootstrap       # connect its skills to this machine's agents
my-skills hooks install   # keep this machine up to date on every git pull
```

## Everyday use

Add a skill: make a folder with a `SKILL.md`, register it, then connect it.

```bash
cd ~/my-agent-skills
mkdir skills/my-skill                       # then write skills/my-skill/SKILL.md
my-skills import skills/my-skill --enable   # register it
my-skills install my-skill --dry-run
my-skills install my-skill --yes
```

See what you have and where it is connected:

```bash
my-skills skills   # every skill and its state in each agent
my-skills doctor   # which registry is in use, and anything that looks wrong
```

Turn a skill off, then apply the change:

```bash
my-skills disable my-skill
my-skills sync --yes
```

Add `--dry-run` to see what a command would do without changing anything.
Commands that write to more than one agent ask for `--yes`.

Registry owners are responsible for skill security. Use dedicated tools such as gitleaks or GitHub secret scanning to check for exposed credentials.

## Use the same skills on several machines

Your registry is a normal Git repo. Push it to a private remote, clone it on
each machine, and run `my-skills hooks install` once in each clone. After that:

- `git pull` connects new skills, updates changed ones, and removes the ones you
  turned off.
- Each commit is checked first, so a broken skill doesn't reach your other
  machines.

## Supported agents

| Agent | Skills folder | Default |
|---|---|---|
| Claude Code | `~/.claude/skills` | on |
| Codex | `~/.agents/skills` | on |
| Antigravity CLI (`agy`) | `~/.gemini/config/skills` | off, turn it on in `my-skills.toml` |

On Windows, links are directory junctions. Antigravity CLI doesn't see
junctions, so connect it with copies there
([how](docs/reference.md#antigravity-cli-agy)).

## More

- [Reference](docs/reference.md): every command, links vs. copies, how my-skills
  finds your registry, safety checks, and where to keep secrets.
- [Architecture](docs/architecture.md): how the code is organized; a diagram-based
  system map in Korean is in [docs/system-map.ko.md](docs/system-map.ko.md).
- [Contributing](CONTRIBUTING.md): building from source and running the tests.
- [Changelog](CHANGELOG.md)

## License

[MIT](LICENSE)
