# HTTP API (Phase B+)

Served by `janus daemon`. Default bind `127.0.0.1:4321`. Reject
`0.0.0.0`, `::`, and non-loopback unless `[daemon.expose]` auth + TLS +
origins are all set ([example.config.toml](../example.config.toml)).

Prefix: `/api/v1`. JSON in/out. Errors: `{ "code": "...", "message": "..." }`
from [errors.md](errors.md).

CLI without a daemon does not need this. When the daemon is up, it is the
only SQLite writer; CLI becomes a client.

## Catalogue

| Method | Path | Notes |
|---|---|---|
| GET | `/roots` | Include `present`, `cold`, `kind`, `writable` |
| POST | `/roots` | Body: path, kind, name, cold. Validates overlap + one fetch root |
| DELETE | `/roots/{id}` | Metadata only; never deletes user files |
| POST | `/roots/{id}/probe` | Re-check presence |
| POST | `/scan` | Body: root ids, `quick`, `no_hash`. Returns `job_id` |
| GET | `/models` | Query: kind, family, root, offline, dups, q, limit |
| GET | `/models/{id}` | Family + variants + files + evidence + provenance |
| GET | `/files` | Query: root, state, hash_state, unknown |
| GET | `/files/{id}` | |
| GET | `/search?q=` | FTS5 + chips |
| GET | `/storage` | Summary + reclaimable |
| GET | `/dups` | Plan only in v1 |
| GET | `/jobs/{id}` | Progress |
| GET | `/jobs` | Recent |

No `/ingest`. Scan and (later) fetch are the only ways bytes enter.

## Identity

| Method | Path | Notes |
|---|---|---|
| POST | `/identify` | Body: file id or path; `deep` opt-in network |
| POST | `/merge` | Body: src, target **or** decline pair. Decline keys are normalized so the lower `family_key` is `family_a_key` and the higher is `family_b_key` (`CHECK family_a_key < family_b_key`). Reversed input is not an error. |
| POST | `/verify` | Body: target; `full` |

## Radar (Phase C)

| Method | Path | Notes |
|---|---|---|
| GET/PUT | `/profiles` | |
| GET/POST/DELETE | `/monitors` | variant must belong to family |
| POST | `/radar` | Sweep; upserts `wanted_items` on `remote_key` |
| GET | `/wanted` | Query: status |

## Fetch (Phase D)

| Method | Path | Notes |
|---|---|---|
| POST | `/fetch` | wanted_id or repo+file; requires sha256; dest validated |
| GET | `/fetch/{id}` | |
| POST | `/fetch/{id}/pause` | |
| POST | `/fetch/{id}/resume` | |

## Rules the handlers must enforce

- Unverified hashes never set `skipped_have_bytes`.
- Fetch dest resolved under the fetch root; reject traversal.
- Existing dest + matching sha256 → done, no overwrite.
- Decline pairs: sort the two family keys before insert/lookup so
  `(B, A)` and `(A, B)` hit the same `declined_merges` row.
- GET is enough for other tools later; do not add write endpoints that
  mutate discovery roots.
