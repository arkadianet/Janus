# Roadmap

Phases are sequential. A phase is closed only when its exit test passes on
the golden hoard. Later work must not start in the same binary as unfinished
identity.

Law: [PRODUCT.md](PRODUCT.md) (done) · [DESIGN.md](DESIGN.md) (how) ·
[docs/backlog.md](docs/backlog.md) (tickets).

---

## Now — Phase D work in tree

Catalogue CLI + loopback daemon/UI + radar + fetch work. Public tags
wait on golden-hoard dogfood on `fedora`. Do not invent those results.

- [x] Architecture (`DESIGN.md`)
- [x] v1 sheet + golden-hoard notes filled (`~/llm/models` on `fedora`)
- [x] License, security note, example config
- [x] CLI / UI / API / schema / family-key case specs
- [x] Synthetic header fixtures (F1 etc.); real tiny public blobs still optional

---

## Phase A — Catalogue (first shippable spine)

CLI only. `janus-core` + `janus`.

| Exit | Test |
|---|---|
| Scan in place | Golden hoard unchanged on disk after `root add` + `scan` |
| Identity | [family-key cases](docs/family-key-cases.md) pass; Unknown is searchable |
| Dups | Report-only; reclaimable uses `(mount_id, dev, ino)` |
| Offline | Unplug → inherit presence; history intact |
| Offline-capable | `scan` / `list` / `show` with no network |
| Portable decisions | `export` / `import` aliases + declined merges |

Commands in scope: `root`, `scan`, `list`, `search`, `show`, `quants`,
`identify` (local), `merge`, `verify`, `dedup --plan`, `storage`, `cold`,
`status`, `doctor`, `export`, `import`.

Out of scope: `daemon` UI, `radar`, `fetch`, `enrich` network, `--apply`.

**Public milestone:** `0.1.0` — CLI catalogue. README three commands work.

---

## Phase B — Browse

`janus daemon` + bundled local UI. Same query engine as the CLI.

| Exit | Test |
|---|---|
| Parity | Every v1 CLI question has a UI page |
| Truth | Badges on derived fields; home counts split inferred |
| First run | Empty → after-scan landing is understandable |
| Bind | Loopback only unless auth + TLS + origin are all on |

**Public milestone:** `0.2.0` — you open the UI instead of the CLI.

---

## Phase C — Radar

Profiles, monitors, wanted / have-offline. No download.

| Exit | Test |
|---|---|
| Publisher | Highest-preference eligible publisher only |
| Revision | Old cutoff cannot hide a new revision |
| Ownership | Unverified / `--quick` files do not satisfy `have_bytes` |
| Privacy | Sweep is opt-in; UI names what leaves the machine |

Unit tests cover these. Golden-hoard radar dogfood on `fedora` is owner-only.

**Public milestone:** `0.3.0` — Wanted tab, still read-only.

---

## Phase D — Fetch

One wanted item → fetch root → verify → ingest.

| Exit | Test |
|---|---|
| Destination | Writes only inside the fetch root; path traversal rejected |
| Digest | Null SHA-256 → fail closed, no install |
| Already owned | Verified blob (incl. offline) refuses without `--force` |
| Restart | Existing dest with matching SHA-256 marked fetched, not overwritten |
| Atomicity | Stage under `<fetch-root>/.janus-partial/`, then fsync + rename |

Unit tests cover these. Golden-hoard fetch dogfood on `fedora` is owner-only.

**Public milestone:** SemVer `1.0.0` — two-faced Janus (radar + fetch).
Not the PRODUCT.md “v1” sheet (that is Phase A+B / `0.2.0`). Still not a
runtime.

---

## Later (after 1.0)

Auto-fetch (default off), more providers, near-dup signatures, read-only
API for other tools, Tauri, collections, VRAM-fit, "open in …" path
handoff, rehash-on-mount, aria2/modfetch as a backend, community
`family_key` maps.

---

## Never

Inference, serving, Hub upload, convert/train, multi-user cloud, mutating
Ollama/LM Studio/HF-cache, unpickling, trending homepage, torrent/`*arr`
indexers. Read DESIGN.md §16 every time a milestone is planned.
