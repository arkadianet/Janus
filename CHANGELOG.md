# Changelog

## Unreleased

### Fixed

- GGUF parse skips large tokenizer/tokens/scores arrays instead of
  aborting the header, so Ollama blobs and Kimi shards still yield
  architecture, basename, and file_type.
- `janus list` PARAMS is human (204.7B) or —; filename shards do not
  invent known params from a shard's tensor count or file size.
- `janus doctor` does not suggest merging instruct ↔ thinking families
  (Kimi Instruct and Thinking stay apart).
- `full_hash` uses a heap buffer so `janus scan` / `identify` do not
  overflow the default 1 MiB Windows thread stack.
- Dedup / storage reclaimable groups by `(mount_id, dev, ino)`, not
  `(dev, ino)` alone across volumes.
- `root add` refuses a volume with no UUID/serial unless `.janus-root`
  already exists or `--accept-marker` / `accept_marker` opts in.

### Added

- First-run `janus` (no args) prints the three catalogue commands plus
  `doctor`. `--help` lists every command.
- `janus verify` and Settings (discover, cold, export/import, probe).
- README build instructions.
- Phase C radar (`0.3.0` work): quality profiles, monitors (variant must
  belong to the family), opt-in HF listings with on-disk cache, sweep
  rules (highest-preference publisher; cutoff scoped per revision),
  `janus profile|monitor|radar|wanted`, `/api/v1/{profiles,monitors,radar,wanted}`,
  Wanted tab. Sweep is read-only. Unverified / `--quick` files do not
  satisfy `have_bytes`. UI names what leaves the machine before a lookup.
- Phase D fetch (SemVer `1.0.0` work): `janus fetch` / `/api/v1/fetch`
  of one open wanted item into the fetch root. Dest path validated
  (no traversal). Null SHA-256 fails closed. Verified-owned blobs
  (including offline) refuse without `--force`. Existing dest with a
  matching digest is marked fetched, not overwritten. Stage under
  `<fetch-root>/.janus-partial/` then fsync + rename. Discovery roots
  are never written.

- Phase B browse: `janus daemon` binds loopback only (`127.0.0.1` / `::1` /
  `localhost`). `0.0.0.0`, `::`, and LAN binds return `api.bind_not_loopback`
  unless `[daemon.expose]` auth + TLS + origins are all set.
- HTTP catalogue API (`/api/v1`) and a bundled local UI (home, library,
  model, unknown, storage, search). Same query engine as CLI `--json`.
- Truth badges on derived fields; home counts split inferred vs known.
- When the daemon is up it is the only SQLite writer; CLI writes refuse
  with the UI URL.

- Remaining family-key goldens: F3, S3, R1/R2/R4/R5, U2/U3, H2.
  Partials are `parse_state=partial`. PyTorch is never unpickled.
  Ollama `sha256-…` blob names are trusted digests (no second full hash).
- `janus identify FILE --name` persists a manual name when the file is
  under a known root.
- Structural ONNX I/O parse and adjacent `config.json` (no pickle).
- Root presence hysteresis (3 consecutive probe failures before
  `present=0`). Cold roots are not polled.
- `janus doctor` prints stable `code` values from `docs/errors.md`.
- `janus root discover` registers Ollama / LM Studio / HF cache as
  read-only discovery roots. Those paths are never written.
- `mount_id` on root add when the OS exposes a real volume UUID/serial.
- `janus scan` with no ROOT scans every present root.
- `--json` on `root ls`, `list`, `status`, `storage`, `doctor`.

- Catalogue CLI: `search`, `merge`, `storage`, `cold`, `root probe`,
  `import`. User merge writes `family_aliases`; rescan follows them (M1).
- Export manifest now matches [docs/export.md](docs/export.md)
  (`format`, `format_version`, `family_aliases`). Import loads families,
  aliases, and declined merges; rejects algo mismatch.
- Windows file identity uses `GetFileInformationByHandle` so reclaim
  math does not collapse every file to inode 0.
- Architecture and product design (`DESIGN.md`).
- v1 product sheet, roadmap, security note, and contributing guide.
- Specs: schema, HTTP API, CLI/UI inventory, family-key cases, fixture
  cases, golden-hoard template, example config, issue templates.
- MIT license.
- Golden-hoard acceptance table filled against the real `~/llm/models`
  hoard (machine `fedora`, 2026-08-18). Offline item left open.

### Changed

- Clarified v1 (Phase A+B / 0.2.0) vs SemVer 1.0.0 (Phase D); fetch
  suppression is verified-owned / `have_bytes` only.

First intended tags: `0.1.0` (Phase A CLI), `0.2.0` (Phase B UI).
Golden-hoard dogfood on `fedora` still open; do not invent results.
