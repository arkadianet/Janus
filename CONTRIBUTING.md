# Contributing

There is no implementation yet. Until Phase A exists, useful work is
specs, fixtures, and golden-hoard notes — not drive-by refactors of
DESIGN.md.

## Order of work

1. Read [PRODUCT.md](PRODUCT.md). If the change is not on the v1 sheet or
   the current [ROADMAP.md](ROADMAP.md) phase, it is Later or Never.
2. Identity bugs beat UI. Wrong families are not polish.
3. New hard cases go in [docs/family-key-cases.md](docs/family-key-cases.md)
   and [fixtures/cases/](fixtures/cases/). Do not "clean up" fixtures.
4. Never-list (DESIGN.md §16) is read at the start of every milestone.

## What to send

- A filled [docs/golden-hoard.md](docs/golden-hoard.md) from a real
  library (paths can be redacted).
- A family-key case: inputs + expected family / variant / Unknown.
- Spec fixes when DESIGN and PRODUCT disagree.

Do not send: downloaders, chat UIs, pickle loaders, trending pages.

## Repo rules

- Author: arkadianet.
- Do not commit `.env`. Use [example.env](example.env) and
  [example.config.toml](example.config.toml).
- Schema changes need a `schema_version` note and a migration story.
- `family_key` is golden-tested. Changing it is a migration, not a tidy-up.

## Code (when it exists)

- Rust for `janus-core` / CLI; Svelte 5 UI compiled at Janus build time.
- `PRAGMA foreign_keys = ON` on every SQLite connection.
- One writer: daemon if running, else CLI advisory lock.
- Tests: family-key cases first, then scan fixtures, then CLI JSON goldens.
