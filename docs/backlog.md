# Backlog

GitHub issues cannot be created from this cloud agent. Use this list when
you open them. One milestone per phase.

## Milestone: Loop 0 (docs)

- [x] DESIGN.md merged
- [x] PRODUCT, ROADMAP, SECURITY, specs, templates
- [x] Fill [golden-hoard.md](golden-hoard.md) on a real machine
- [ ] Decide public crate/binary name if `janus` is too generic
- [x] Synthetic header fixtures (real tiny public blobs still optional)

## Milestone: 0.1.0 Phase A

- [x] A1 janus-core: SQLite (`docs/schema.sql`), `PRAGMA foreign_keys=ON`
- [x] A2 `root add/ls/rm/probe` + overlap / one-fetch (`mount_id` when OS exposes one)
- [x] A3 Walk + header parse (GGUF, safetensors, ONNX I/O, config.json adjacency)
- [x] A4 Hash pipeline (walk gate ≠ proof; one-pass BLAKE3+SHA-256; Ollama digest reuse)
- [x] A5 family_key + cases F1–U3, M1–M2 (all listed goldens have fixtures)
- [x] A6 Offline presence inherited; hysteresis; cold
- [x] A7 CLI list/search/show/storage/dedup --plan
- [x] A8 Unknown + identify (local + persist under a known root)
- [x] A9 export/import ([export.md](export.md)) — decisions + families; no `mount_id` requires `--accept-marker`
- [x] A10 doctor + error codes ([errors.md](errors.md))
- [x] A11 Discovery sources read-only (Ollama, LM Studio, HF cache)
- [ ] A12 Golden hoard dogfood on `fedora`; freeze family_key_algo=1 (owner)

## Milestone: 0.2.0 Phase B

- [x] B1 daemon single-writer + loopback bind
- [x] B2 API [api.md](api.md) catalogue endpoints
- [x] B3 UI: home, library, model, unknown, storage, search
- [x] B4 Truth badges + first-run ([first-run.md](first-run.md), [ui-inventory.md](ui-inventory.md))
- [x] B5 Second OS smoke (developed and tested on Windows)
- [ ] Golden hoard dogfood on `fedora` (owner) before tagging `0.2.0`

## Milestone: 0.3.0 Phase C

- [x] C1 profiles + monitors (variant belongs to family)
- [x] C2 HF availability listings + cache
- [x] C3 Sweep rules (publisher order, revision-scoped cutoff)
- [x] C4 wanted UI; have-offline ≠ missing
- [x] C5 Privacy copy before any lookup
- [ ] Golden hoard radar dogfood on `fedora` (owner)

## Milestone: 1.0.0 Phase D

- [x] D1 fetch root only; dest_rel_path validation
- [x] D2 fail closed without sha256
- [x] D3 in-root staging + fsync + rename
- [x] D4 restart reconcile
- [x] D5 `--force` vs already-owned (incl. offline)
- [ ] Golden hoard fetch dogfood on `fedora` (owner)

## Later / icebox

Auto-fetch, ModelScope/Civitai, near-dup, Tauri, VRAM-fit, launch
handoff, aria2 backend, community family maps.

## Never (do not file as features)

Chat, serve, upload, convert, pickle, mutate app caches, torrent indexers.
