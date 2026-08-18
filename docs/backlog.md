# Backlog

GitHub issues cannot be created from this cloud agent. Use this list when
you open them. One milestone per phase.

## Milestone: Loop 0 (docs)

- [x] DESIGN.md merged
- [x] PRODUCT, ROADMAP, SECURITY, specs, templates
- [ ] Fill [golden-hoard.md](golden-hoard.md) on a real machine
- [ ] Decide public crate/binary name if `janus` is too generic
- [ ] Add tiny public header blobs when Phase A starts (see fixtures)

## Milestone: 0.1.0 Phase A

- A1 janus-core: SQLite (`docs/schema.sql`), `PRAGMA foreign_keys=ON`
- A2 `root add/ls/rm/probe` + overlap / one-fetch / mount_id rules
- A3 Walk + header parse (GGUF, safetensors, ONNX I/O, config.json)
- A4 Hash pipeline (walk gate ≠ proof; one-pass BLAKE3+SHA-256; Ollama digest reuse)
- A5 family_key + cases F1–U3, M1–M2
- A6 Offline presence inherited; hysteresis; cold
- A7 CLI list/search/show/storage/dedup --plan
- A8 Unknown + identify (local)
- A9 export/import ([export.md](export.md))
- A10 doctor + error codes ([errors.md](errors.md))
- A11 Discovery sources read-only (Ollama, LM Studio, HF cache)
- A12 Golden hoard dogfood; freeze family_key_algo=1

## Milestone: 0.2.0 Phase B

- B1 daemon single-writer + loopback bind
- B2 API [api.md](api.md) catalogue endpoints
- B3 UI: home, library, model, unknown, storage, search
- B4 Truth badges + first-run ([first-run.md](first-run.md), [ui-inventory.md](ui-inventory.md))
- B5 Second OS smoke (Windows placeholders)

## Milestone: 0.3.0 Phase C

- C1 profiles + monitors (variant belongs to family)
- C2 HF availability listings + cache
- C3 Sweep rules (publisher order, revision-scoped cutoff)
- C4 wanted UI; have-offline ≠ missing
- C5 Privacy copy before any lookup

## Milestone: 1.0.0 Phase D

- D1 fetch root only; dest_rel_path validation
- D2 fail closed without sha256
- D3 in-root staging + fsync + rename
- D4 restart reconcile
- D5 `--force` vs already-owned (incl. offline)

## Later / icebox

Auto-fetch, ModelScope/Civitai, near-dup, Tauri, VRAM-fit, launch
handoff, aria2 backend, community family maps.

## Never (do not file as features)

Chat, serve, upload, convert, pickle, mutate app caches, torrent indexers.
