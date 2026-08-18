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
this build, import may load aliases/declines and must not silently rewrite
keys.

`enrichments` may be omitted (rebuildable, marked `external`).

## Import

- Never overwrite user files on disk.
- Never assign a fetch write to a catalogue path.
- Reconcile roots by `mount_id` first, path second — not path alone if
  `mount_id` is missing (see DESIGN.md root identity).
- Duplicate `remote_key` / alias rows upsert.
