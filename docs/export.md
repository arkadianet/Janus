# Export / import

A file-list dump is not a backup. The portable artefact is **identity
decisions + catalogue facts**.

## Manifest (`janus export`)

JSON. Versioned.

```json
{
  "format": "janus.export",
  "format_version": 1,
  "family_key_algo": "1",
  "exported_at": "2026-08-18T00:00:00Z",
  "roots": [],
  "blobs": [],
  "files": [],
  "families": [],
  "family_aliases": [],
  "declined_merges": [],
  "revisions": [],
  "variants": [],
  "file_roles": [],
  "evidence": [],
  "provenance": [],
  "profiles": [],
  "monitors": [],
  "wanted": [],
  "tags": []
}
```

Required on import: `format`, `format_version`, `family_key_algo`,
`family_aliases`, `declined_merges`. If `family_key_algo` does not match
this build, **reject** the manifest (`export.algo_mismatch`). Do not
activate aliases or declined merges by matching raw strings across
algorithms. Load those decisions only after an explicit key-migration
step that rewrites them into the current algo.

`enrichments` may be omitted (rebuildable, marked `external`).

## Import

- Never overwrite user files on disk.
- Never assign a fetch write to a catalogue path.
- Reconcile roots by `mount_id` first. Reject a root that has no stable
  `mount_id` unless a validated `.janus-root` marker is present or the
  user explicitly re-registers that root (`root.no_mount_id`). Never
  associate such a root by path alone.
- Duplicate `remote_key` / alias rows upsert.
