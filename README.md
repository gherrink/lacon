# lacon

A CLI tool that filters and rewrites bash command output to reduce token consumption by AI coding assistants.

> **Status:** in design. No installable artifact yet. See `docs/` for the design.

When Claude Code (or another coding assistant) runs `pnpm install`, the tool burns thousands of tokens on progress bars, deprecation warnings, and other noise that doesn't help the model do its job. `lacon` sits between the assistant and the shell — via the assistant's hook system — and applies configurable filter rules to strip noise while preserving signal, especially on errors.

## Documentation

- [Vision](docs/vision.md) — what we're building and why
- [v1 scope](docs/v1-scope.md) — what's in the first release
- [Backlog](docs/backlog.md) — deferred ideas
- [Architecture](docs/architecture.md) — system shape
- [Open questions](docs/open-questions.md) — known risks and unknowns
- [Bundled rules roadmap](docs/bundled-rules-roadmap.md) — which filters we plan to ship
- Specs:
  - [Filter rule schema](docs/specs/filter-rule-schema.md)
  - [Tracking data model](docs/specs/tracking-data-model.md)
- [Decisions](docs/decisions/) — architecture decision records

## License

[MIT](LICENSE)
