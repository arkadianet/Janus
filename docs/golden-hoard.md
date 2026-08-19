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

## How to dogfood (owner)

This agent cannot see `~/llm/models` on `fedora`. Do not invent scan
results. Run these yourself and tick the handwritten tables above.

```bash
cargo test --workspace          # includes fixture_cases_pass
cargo run -p janus -- cases     # same goldens via the CLI
janus root add ~/llm/models
janus root discover             # Ollama / LM Studio / HF cache if present
janus scan                      # all present roots; does not move files
janus list
janus doctor
```

`family_key_algo=1` is frozen in `janus-core`. Changing it is a migration.

## After scan (trust checks)

- [ ] Folders on catalogue roots have no new Janus files (unless you opted
      into `.janus-root`).
- [ ] Home / `list` shows inferred counts separately.
- [ ] `--quick` did not mark unverified files as `have_bytes`.
- [ ] Discovery roots were not written.

Date filled: 2026-08-18  Machine: fedora

## Windows dogfood (filled 2026-08-19, machine `Chace` / win32)

Catalogue-only. No fetch. No `.janus-root` markers written (D: has a
volume serial). Discovery root was not written.

### Roots added

| Name | Path | Kind | Notes |
|---|---|---|---|
| ollama | `D:\LocalAI\OllamaModels` (`OLLAMA_MODELS`) | discovery | 53 blobs + 13 manifests. Large blobs are GGUF v3. Never written. |
| Models | `D:\LocalAI\Models` | internal | Kimi-K2 Instruct + Thinking Unsloth `UD-TQ1_0` shards (~365 GB). |
| ComfyUI | `D:\LocalAI\ComfyUI\models` | internal | Flux.1 schnell + CLIP-L + T5-XXL + Flux VAE safetensors. |

Not present on this PC: HF cache, LM Studio models dir,
`C:\Users\Chace\.ollama\models` blobs (empty; env points at D:),
`E:\Archive\Models` (empty folder — not registered).

`janus root discover` registered ollama only. Volume UUID present; no
`--accept-marker`. Overlap/permission errors: none.

### `janus scan --quick` (19s)

| Root | seen | unsupported | unverified | families_new |
|---|---|---|---|---|
| ollama | 66 | 66 | 13 | 0 |
| Models | 24 | 15 | 24 | 2 |
| ComfyUI | 42 | 38 | 42 | 4 |

Status: 6 families (all name-inferred), **1.59 TB** indexed, 79 files
not full-hashed, 0 unattached content-role names, 3/3 roots present.

Ollama GGUF blobs fail header parse (`gguf: array too large` — tokenizer
arrays over the 4096 cap) so they stay unsupported; manifests are not
models. Filename `sha256-<hex>` is trusted as a full digest (no rehash).

Kimi GGUF shards hit the same header cap; families still appear from
**filename**. Doctor suggested merging Instruct ↔ Thinking (shared
tokens, score 1.00) — those must stay apart.

### Catalogue (`janus list`)

| Display name | Kind | Notes |
|---|---|---|
| kimi-k2-instruct-tq1-0~ | unknown | 5 shards, 180.7G, Models. Quant leaked into family name. |
| kimi-k2-thinking-tq1-0~ | unknown | 6 shards, 183.7G, Models. Distinct from Instruct. |
| flux1-schnell~ | unknown | 22.1G, ComfyUI unet. |
| t5xxl-fp8-e4m3fn~ | unknown | 4.6G, ComfyUI clip. |
| clip-l~ | unknown | 234.7M, ComfyUI clip. |
| flux-vae~ | unknown | 159.9M, BF16, ComfyUI vae. |

`list` PARAMS column prints raw counts as `N.0B` (e.g. CLIP-L
`123060480.0B`) — display bug, not a second family.

### Ollama library on disk (not in `list` — parse unsupported)

`deepseek-r1/70b`, `deepseek-v3`, `gpt-oss`, `llama3.1/8b`,
`llama4/maverick`, `llama4/scout`, `nemotron-3-nano/30b`,
`nemotron-3-super`, `qwen2.5-coder` 7b/14b/32b, `qwen3` 8b/235b.

Largest blobs (GGUF magic): 377G, 228G, 132G, 81G, 63G, 40G, 23G, 18G.

### README three commands

`janus root add PATH` / `janus scan` / `janus list` work on this PC
(debug `target\debug\janus.exe`). `--quick` populated the family table
in 19s. Full `janus scan` (BLAKE3+SHA-256 of the ~365 GB Kimi shards)
was still running after 4h at ~40 MB/s on D: — leave it; do not kill.

### Windows bug found this pass

`full_hash` used a 1 MiB stack array and overflowed the default Windows
thread stack (`janus identify` / full `scan`). Fixed: heap `Vec`.

Date filled: 2026-08-19  Machine: Chace (Windows)