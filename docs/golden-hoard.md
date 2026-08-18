# Golden hoard (filled 2026-08-18, machine `fedora`)

v1 is not done until these handwritten answers match `janus scan` on
*your* disks. This agent cannot see those disks. Redact paths if you
publish the notes.

Copy the tables. Do not invent a pretty library — use the mess.

## Roots

| Name | Path | Kind | Can unplug? | Notes |
|---|---|---|---|---|
| llm-models | `~/llm/models` | internal | no | 324 GB, 30 files, all GGUF. The real hoard. |
| ollama | `$OLLAMA_MODELS` else `~/.ollama/models` | discovery | no | `qwen2.5-coder` 14b + 32b, 27 GB blobs. Never written. May hardlink-count for reclaim math. |
| fetch | — | fetch | — | Not yet registered. Phase D; only place Janus may write. |
| *(gap)* | no NAS / removable currently mounted | — | — | PRODUCT item 6 (real unplug) stays **open** until a removable root exists. A mock `present=0` is a unit test only, not acceptance. |

## Expected families

Filename read is an `inferred` hypothesis. GGUF headers may still split or
join. One row per family.

| Display name | Files (rel paths) | Must be one family? | Publishers to keep distinct |
|---|---|---|---|
| DeepSeek-R1-Distill-Qwen-32B | `DeepSeek-R1-Distill-Qwen-32B-Q4_K_M.gguf`, `DeepSeek-R1-Distill-Qwen-32B-Q5_K_M.gguf` | yes — one family, two quants | — |
| Qwen3-Coder-30B-A3B-Instruct-UD | `Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL.gguf`, `Qwen3-Coder-30B-A3B-Instruct-UD-Q6_K_XL.gguf` | yes — one family, two quants | `UD` = Unsloth Dynamic (publisher/quant flavour), not a separate family |
| Qwen3.6-27B | `Qwen3.6-27B-UD-Q4_K_XL.gguf`, `Qwen3.6-27B-UD-Q5_K_XL.gguf` | yes — one family, two quants | — |
| gpt-oss-120b | `gpt-oss-120b-UD-Q4_K_XL-00001-of-00002.gguf`, `gpt-oss-120b-UD-Q4_K_XL-00002-of-00002.gguf` | yes — one family, one variant, roles `shard` (R2) | — |
| Qwen3.8-27B-Uncensored-HauhauCS-Aggressive | `Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf` (weights) + `Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-FastMTP-32K.gguf` (role `sidecar`) + `mmproj-Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-BF16.gguf` (role `mmproj`) | yes — companions ride this family as roles, NOT extra quants/families | HauhauCS |
| Qwen3.8-27B | `Qwen3.8-27B-Q4_K_M.gguf` | yes | official-style; stays apart from the Uncensored spin-off (S3) |
| Qwen3.6-35B-A3B | `Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf` | yes | apart from Qwen3.5-35B-A3B (name/version, not a coarse param bin) |
| Qwen3.5-35B-A3B | `Qwen3.5-35B-A3B-UD-Q4_K_XL.gguf` | yes | apart from Qwen3.6-35B-A3B |
| Nemotron-3-Nano-30B-A3B | `Nemotron-3-Nano-30B-A3B-UD-Q4_K_XL.gguf` | yes | apart from Coder-30B family |
| Devstral-Small-2-24B-Instruct | `Devstral-Small-2-24B-Instruct-2512-UD-Q4_K_XL.gguf` | yes | — |
| gemma-4-26B-A4B-it | `gemma-4-26B-A4B-it-UD-Q4_K_XL.gguf` | yes | — |
| GLM-4.7-Flash | `GLM-4.7-Flash-UD-Q4_K_XL.gguf` | yes | — |
| bge-reranker-v2-m3 | `bge-reranker-v2-m3-Q8_0.gguf` | yes — kind `rerank` | — |
| nomic-embed-text-v1.5 | `nomic-embed-text-v1.5.Q8_0.gguf` | yes — kind `embeddings` | — |

## Must stay apart

| File A | File B | Why |
|---|---|---|
| `Qwen3.8-27B-Q4_K_M.gguf` | `Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf` | same arch is not the same model — different model (S3). Never one family. |
| `Qwen3.5-35B-A3B-UD-Q4_K_XL.gguf` | `Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf` | name/version difference; no coarse 27–35B bin. |
| `Qwen3.6-27B-UD-Q4_K_XL.gguf` | `Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf` | different params (27B vs 35B-A3B); no coarse 27–35B bin. |
| `DeepSeek-R1-Distill-Qwen-32B-*.gguf` | any 30B-A3B-class family | MoE (dense?) vs A3B — params identity differs; suggestion-only merge if human says so (S1). |
| FastMTP-32K file | any quant | it is a companion (role `sidecar`), never a variant on the quant ladder. |

## Unknown inbox

Files that should be catalogued and searchable *before* they have a name.

| Path | Why unknown |
|---|---|
| `mmproj-F16.gguf` | orphaned projector — no parent model on disk. Unattached, role `mmproj`, searchable. Do **not** invent a parent family for it; stays in inbox until the user attaches it. |

Excluded from the inbox on purpose:

| Path | Why |
|---|---|
| `qwen3.8-server.log` | not a model. Skip / `unsupported` (or filtered). No identify attempt. |

## Byte-identical copies

| Blob (size or note) | Paths | Same inode? | Reclaimable? |
|---|---|---|---|
| none found by size (all file sizes distinct) | — | — | recheck after full BLAKE3+SHA-256 pass |

## Offline check

**Status: open — no removable root exists on this machine yet.**

1. Scan with all roots present. Note family X locations.
2. Unplug / unmount root R.
3. Expect: R `present=0`; family X still listed; files not mass-updated to
   `missing`; verified blobs still "owned, offline."
4. Plug back in. `probe` / `scan` reattaches without losing provenance.

A mock `present=0` is implemented as a **unit test**. It does not satisfy
PRODUCT item 6, which requires a real unplug/mount of a removable root. The
checkbox stays off until that run happens.

## After scan (trust checks)

- [ ] Folders on catalogue roots have no new Janus files (unless you opted
      into `.janus-root`).
- [ ] Home / `list` shows inferred counts separately.
- [ ] `--quick` did not mark unverified files as `have_bytes`.
- [ ] Discovery roots were not written.

Date filled: 2026-08-18  Machine: fedora