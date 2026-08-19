# UI inventory (Phase B)

Local web UI at `http://127.0.0.1:4321`. Same query engine as the CLI
([cli-ux.md](cli-ux.md) JSON). No signup. Dark first. No trending.

If a screen is not in this list, it is not v1.

## First run

See [first-run.md](first-run.md). Empty home: one action — add a root.
After scan: the landing numbers, with inferred split.

## Home

| Block | Must show |
|---|---|
| Counts | Families, files, bytes; **inferred family count separate** |
| Reclaimable | Report-only figure; not a delete button |
| Unknown | Count → inbox |
| Offline | Grey roots + last seen |
| Wanted | Hidden until Phase C; then open count only |
| Recent | Newly seen files this scan |

Not on home: download queue, recommended models, like counts.

## Library

- Group by family. Variant ladder is the hero (quants horizontally).
- Filters: kind, size, params, quant, source, licence, tag, offline-only,
  duplicates-only. `fits-vram` is Later.
- Grid or compact table. Dense, readable.

## Model

- Identity card + truth badges on every derived field.
- Variant ladder; publisher visible (bartowski ≠ official).
- Instances per root: path, present/offline, last seen.
- Provenance timeline.
- Actions in v1: reveal (if present), verify, identify, merge suggestion.
- Phase C: "radar this family." Phase D: fetch only on an **open** wanted
  item with a digest.

## Unknown

Inbox. Each row: path, size, parse_state, hash_state. Inline identify:
reparse → optional opt-in lookup → type a name. Accept / decline
suggestions. Searchable before naming.

## Storage

Treemap or bars: root → family. Dedup panel: reclaimable by blob group.
No apply in Phase A/B. Offline roots included in "what exists" bytes, not
in "I can free this now."

## Duplicates

Blob-exact groups. Same-bytes different name. Near-dup is Later.
Reclaimable math: unique `(mount_id, dev, ino)`.

## Search

`Ctrl/Cmd-K`. Chips: `quant:`, `params:`, `offline`, `wanted` (C+),
`have-bytes` (verified only).

## Radar / Wanted (Phase C)

Monitored families, last sweep, open / have-offline / fetched.
Have-offline is not missing. Privacy copy + confirm before sweep.
Confirm fetch (D) is not auto; fetch only on an open wanted item with a digest.

## Chrome

- Job/progress: scan, hash, later fetch (poll or SSE).
- `doctor` warnings (placeholders, missing fetch root, no `mount_id`).
- Settings: roots, cold, discover, export/import, doctor. Enrichment stays off until Sweep confirm.

## Accessibility / polish bar

- Keyboard through search and tables.
- No information only in color (use `~` / badge text).
- Empty states are instructions, not illustrations.
