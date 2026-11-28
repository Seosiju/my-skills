# Roadmap & Future Ideas

Ideas under consideration for future versions of `my-skills`. Items here are not committed schedules — they are recorded so design discussions and user feedback have a home.

## Multi-registry support (v2.0 candidate)

- **Problem**: Today `my-skills` connects one active registry per machine. That works cleanly for a personal registry or a single shared team/company org registry, but a user cannot attach both a company shared skill repo and a personal skill repo to the same machine at once.
- **Idea**: Allow mounting multiple registries (for example, an org/team registry alongside a personal registry) with clear precedence rules when skill names collide, isolated Git hooks per registry, and clear provenance in `my-skills skills` and `my-skills doctor`.
- **Status**: Deferred until real multi-registry demand arises; keep the single-registry v1.x UX simple in the meantime.
