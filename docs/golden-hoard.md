# Golden hoard (fill this in)

v1 is not done until these handwritten answers match `janus scan` on
*your* disks. This agent cannot see those disks. Redact paths if you
publish the notes.

Copy the tables. Do not invent a pretty library — use the mess.

## Roots

| Name | Path | Kind | Can unplug? | Notes |
|---|---|---|---|---|
| | | internal / nas / removable / discovery | yes/no | |
| | | | | Ollama / LM Studio / HF cache if you have them |
| | | fetch (optional for v1) | | Only place Janus may write, Phase D |

## Expected families

List groups you already know by eye. One row per family.

| Display name | Files (rel paths) | Must be one family? | Publishers to keep distinct |
|---|---|---|---|
| e.g. Qwen3-Coder-30B | `qwen3-coder-q4.gguf`, `…-Q5_K_M.gguf`, `…-Q8.gguf` | yes | bartowski vs official if both exist |

## Must stay apart

| File A | File B | Why |
|---|---|---|
| | | e.g. 30B-A3B vs 32B, or two different 8B Llamas |

## Unknown inbox

Files that should be catalogued and searchable *before* they have a name.

| Path | Why unknown |
|---|---|
| | empty metadata, random name, unsupported format |

## Byte-identical copies

| Blob (size or note) | Paths | Same inode? | Reclaimable? |
|---|---|---|---|
| | | yes/no | no if hardlink to Ollama |

## Offline check

1. Scan with all roots present. Note family X locations.
2. Unplug / unmount root R.
3. Expect: R `present=0`; family X still listed; files not mass-updated to
   `missing`; verified blobs still "owned, offline."
4. Plug back in. `probe` / `scan` reattaches without losing provenance.

## After scan (trust checks)

- [ ] Folders on catalogue roots have no new Janus files (unless you opted
      into `.janus-root`).
- [ ] Home / `list` shows inferred counts separately.
- [ ] `--quick` did not mark unverified files as `have_bytes`.
- [ ] Discovery roots were not written.

Date filled: ________  Machine: ________
