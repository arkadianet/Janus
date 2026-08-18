# Family-key cases

`family_key` is a pure function. Changing it is a migration. Each case is
a fixture when coding starts (`fixtures/cases/*.toml`).

Algorithm (DESIGN.md §8):

```text
family_key = slug(display_name) + "|" + arch + "|" + params_identity
params_identity = known total + active, or unk
```

Coarse bins (`27-35B`) are not in the key. No silent merges. Publisher is
a variant axis, not a stopword. Local revision id is `local:<full-blake3>`.

Truth: KV / tensors = `known`. Filename = `inferred`.

---

## Must merge (one family, several variants)

### F1 — Three Qwen GGUFs (the brief)

Requires GGUF header fixtures (`fixtures/cache/F1/…`). Filename-only
input is not enough: `qwen3-coder-q4.gguf` infers `Q4`, not `Q4_K_M`.
Assert `Q4_K_M` / `Q5_K_M` / `Q8_0` only from `general.file_type` (known).

| File | Expect (from header) |
|---|---|
| `qwen3-coder-q4.gguf` | same family; quant `Q4_K_M` (header), not filename `Q4` |
| `Qwen3-Coder-30B-Q5_K_M.gguf` | variant Q5_K_M |
| `Qwen3-Coder-30B-Q8.gguf` | variant Q8_0 |

Same `known` basename + arch + params. Quants differ. Publisher `unknown`
unless KV/filename says otherwise. Do not run the Q4_K_M assertion until
the header fixtures are present.

### F2 — Same bytes, two names

Same BLAKE3 on two paths → one blob, two files, one variant. Not two
families.

### F3 — HF cache snapshot + renamed copy in `~/models`

Same blob. One file may have rich HF provenance; the other inferred name.
Still one family. Provenance stays on the file that earned it.

---

## Must not silent-merge (two families + suggestion, or distinct keys)

### S1 — Qwen3-30B-A3B vs Qwen3-32B

Different known `params_identity` (MoE active vs dense 32B). Different
keys. Merge suggestion only if a human says so.

### S2 — Two unrelated 8B Llamas

Same coarse size, different `general.basename` / name. Two families. Do
not Jaccard them together because both are "llama 8b."

### S3 — instruct vs base vs thinking vs coder

Same base weights family *may* share a family if `known` basename agrees
and subflavour differs — those are **variants** (`subflavour`), not extra
families. If `known` names disagree (`Foo-Instruct` vs `Bar-Base`), two
families + suggestion.

### S4 — bartowski vs official vs local convert, same quant

**One family, three variants** (publisher axis). Do not strip
`bartowski-` and collapse to one variant.

---

## Structure / roles (not extra families)

### R1 — Sharded safetensors + `index.json`

One variant, many files, roles `shard` / `config`.

### R2 — Split GGUF `*-00001-of-00003`

One variant, roles `shard`.

### R3 — mmproj beside the LLM

Same family (or attached variant), role `mmproj`. Not its own family.

### R4 — LoRA next to a base

Role `lora` if a base is identifiable; else Unknown adapter, not a fake
LLM family.

### R5 — Diffusers dir (`model_index.json`)

`kind=diffusion`, structural in v1. Do not pretend GGUF-level identity.

---

## Unknown (first-class)

### U1 — `random.safetensors` empty `__metadata__`

Hashed, searchable, Unknown inbox. No invented family name.

### U2 — `.bin` / `.pt` / `.pth`

Detected by adjacency + `config.json` only. **Never unpickle.** Often
Unknown / unsupported for weights.

### U3 — Partial (`.part`, `.crdownload`, fetch staging)

`parse_state=partial`. Not a model. Not Unknown-as-family.

---

## Scan / ownership (not family_key, but same goldens)

### H1 — `--quick` only

Files `hash_state=none`. Grouping from headers/names is allowed.
`have_bytes` / fetch-suppression forbidden.

### H2 — Ollama `sha256-…` blob name + matching size

Trusted provider digest. May set `hash_state=full` without a second
full-file hash.

### H3 — In-place overwrite, timestamps restored

`size,mtime,ino` match must **not** reuse the old BLAKE3 until
`change_gen` or partial-hash confirms.

---

## User decisions

### M1 — User merge

Writes `family_aliases`. Survives export/import. Rescan respects it.

### M2 — User decline

Writes `declined_merges` (canonical pair + `algo_version`). Does not use
`family_aliases`. Survives export/import. Sweep must not re-nag the same
pair for that algorithm version.
