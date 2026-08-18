# Error catalog

Stable `code` values for CLI `--json` and the HTTP API. Human `message` may
change; `code` must not.

| Code | When | User should |
|---|---|---|
| `root.not_found` | Path missing at add/probe | Check mount |
| `root.no_mount_id` | No volume UUID/serial and no `.janus-root` | Opt in to marker or re-register |
| `root.overlap` | New path is ancestor/descendant of an existing root | Pick a non-nested path |
| `root.fetch_exists` | Second `--kind fetch` | Remove or change the existing fetch root |
| `root.not_writable` | Fetch/write aimed at a catalogue or discovery root | Use the fetch root |
| `root.discovery_readonly` | Any write/dedup-apply on Ollama/LM Studio/HF cache | Don't |
| `scan.io` | Read failed mid-walk | Check disk; job is resumable |
| `scan.placeholder` | Skipped unhydrated cloud file | Not an error if skipped; warn in doctor |
| `parse.unsupported` | Format not handled; file still catalogued | Identify later |
| `parse.header_too_large` | Safetensors/GGUF cap hit | File stays unsupported |
| `parse.pickle_refused` | `.pt/.bin/.pth` weights path | Never unpickle |
| `hash.unverified` | Caller asked for have_bytes on `hash_state=none` | Full scan / trusted digest |
| `identity.merge_declined` | Pair is in `declined_merges` | Stop nagging |
| `export.incomplete` | Missing aliases/declines in payload | Refuse import |
| `api.bind_not_loopback` | `--api 0.0.0.0` without expose trio | Bind localhost |
| `radar.variant_family_mismatch` | monitor.variant not in monitor.family | Reject monitor |
| `wanted.no_sha256` | Fetch/install without digest | Fail closed; no bytes |
| `fetch.path_invalid` | dest_rel_path absolute, `..`, UNC, drive, symlink parent | Reject |
| `fetch.already_owned` | Verified blob exists (maybe offline) | `--force` to duplicate |
| `fetch.dest_mismatch` | Existing dest failed checksum | Do not overwrite; doctor |
| `fetch.cross_fs` | Staging not on the fetch-root filesystem | Internal bug; stage in-root |
| `network.disabled` | Enrich/radar/fetch while offline / provider off | Expected |

Doctor aggregates `root.*`, `scan.placeholder`, `hash.unverified` counts
without failing the process.
