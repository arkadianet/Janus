# Janus — Architecture & Product Design

**Working title:** Janus — the Plex *and* Sonarr of your local AI model hoard.

**One-line definition:** A local-first library for the AI model files you
already own, with a second face that watches the outside world and can fetch
what you are missing — without becoming another downloader.

Janus is named for the two faces:

| Face | Looks | Answers |
|---|---|---|
| **Inward (catalogue)** | Your disks, including ones in a drawer | What do I own, what is it, where, how much space, what's duplicated? |
| **Outward (radar + fetch)** | HF / other providers, against that catalogue | What's new, what quants am I missing, do I already have these bytes, grab this one. |

This document is opinionated. Where there are trade-offs, it picks a side and
says why.

---

## 1. Product definition

Janus is **a catalogue first, an acquirer second, a runtime never**.

It is the librarian for a personal collection of AI model files. Fetch exists
so the library can grow on purpose. Inference, chat, and serving do not.

**The media-stack analogy (this is the product):**

| Media stack | Janus |
|---|---|
| Plex / Jellyfin | Catalogue + browse + search of what you already have |
| Sonarr / Radarr | Radar (wanted / missing / cutoff) + fetch of a specific release |
| TMDB match | Local identity + optional enrichment, with truth levels |
| Quality profile (1080p WEB-DL, cutoff) | Quant × format × publisher profile (Q4_K_M GGUF, bartowski, cutoff) |
| Root folder (library) vs download client | Catalogue roots (read) vs one fetch root (write) |
| "Do I already have this episode?" | "Do I already have these bytes — including on an offline drive?" |
| Does not transcode unless asked | Does not run inference. Ever, in this design. |

**Target user:** someone who accumulates terabytes of models across drives and
tools, wants to see the collection, and sometimes wants the next quant or the
new revision without opening five browser tabs and guessing whether they
already downloaded it.

**Core promise:** figure out what you have, tell the truth about what is known
vs guessed, and only then help you get what you don't have — without moving a
single existing file, without an account, and without downloading a file the
catalogue already knows.

**v1 honesty:** Janus is an **LLM / GGUF catalogue** that also handles
well-behaved safetensors and ONNX. Diffusion, ComfyUI, and Civitai piles are
catalogued as files and can sit in Unknown. They are not the v1 identity
target. The pitch is not "Plex for every AI artefact on day one."

---

## 2. Existing-project landscape

Researched Aug 2026. Four archetypes, plus the media-stack pattern we steal.

### A. Runtimes and downloaders that own their models
They download into their own store and manage *what they downloaded*. They
cannot catalogue a pre-existing 15 TB scatter.

- **Ollama, LM Studio, Jan, GPT4All** — runtime + per-app cache.
- **modfetch, modeldock, ggufy, lilbee, llamaUI, hfdesk, EZmodL, …** —
  download-first, often runtime-flavoured. modfetch in particular is a serious
  fetch/verify/TUI tool (HF, CivitAI, ModelScope, resume, checksums). Janus
  must not try to out-download it.
- **modelshelf (Rust SDK/CLI)** — closest neighbour: scans Ollama / LM Studio /
  HF cache / Jan / GPT4All / custom dirs, content-hash dedup, journaled
  hardlink reclaim, GGUF header reader, stat-cache hashing, HF update checks.
  It is a shared on-disk registry for apps, not a browse-first hoarder
  catalogue, and not an offline-drive / provenance / radar UI. **Do not fork
  it.** Treat it as a possible *library* later (hash/scan helpers), not as the
  product.

### B. Scanners and disk cleaners
- **ai-model-scanner, dehoard** — find / report / distinguish Q4 ≠ Q8.
  No persistent identity, no offline library, no UI of record.
- **File Hunter** — catalogues files across offline USB/backup drives,
  three-gate hashing, SQLite WAL, browse-while-unplugged. This is the storage
  *substrate* Janus needs. It does not know what a quantization is.

### C. Hub mirrors
HuggingHack, KohakuHub, hf-local-hub, CSGHub, … — private Hub replacements.
Not a personal catalogue of existing junk.

### D. UX we steal
- **MeshVault / Modelist / STLHub** — local SQLite, index-in-place, hash
  dedupe, browser-first hoard.
- **Latent-Model-Organizer** — safe, undoable moves (reuse later, not MVP).
- **Sonarr / Radarr / the *arr suite** — monitor + quality profile + wanted /
  missing / cutoff + fetch into a library root. This is the outward face.
  Janus is *partly* an *arr: the mental model, not the usenet/torrent stack.

### What Janus uniquely combines
1. Model-aware identity on **arbitrary existing folders**, with truth levels.
2. Non-destructive catalogue of roots you do not reorganise.
3. Multi-drive instances, including **offline / removable** storage.
4. Provenance with labelled confidence.
5. Radar: availability vs the catalogue (including offline-owns-it).
6. Fetch of a **wanted variant** into one writable root — never a browse-and-
   download home screen.

**Gap:** the ecosystem has downloaders, scanners, and hub mirrors. It lacks a
*catalogue of the present* whose second job is *fill this hole on purpose*.

---

## 3. Collector lifecycle

Six months after "Qwen just dropped, downloading":

| Question | Who answers today | Janus |
|---|---|---|
| I have files here and here | scanners | Catalogue |
| Download this, run that | runtimes / modfetch | Not the home screen |
| Private Hub | mirrors | Out of scope |
| What do I own, what is it, where, from where, what's duplicated, what's on the drive in the drawer? | nobody together | Inward face |
| I have Q4; HF has Q5 and Q8; I already have Q8 on the NAS that's unplugged — don't fetch it again | nobody | Radar |
| Get me that Q5 into `~/models/inbound` and catalogue it | downloaders, blind to the hoard | Fetch |

Fetch is a **hook on a hole in the catalogue**, not how you discover Janus.

---

## 4. Domain model (locked)

Always ask: *which layer is this fact attached to?*

```
FAMILY       Qwen3-Coder-30B                         the show
  REVISION   hf:Qwen/Qwen3-Coder@e3f2a1c | local:…   the season
    VARIANT  gguf · Q4_K_M · instruct · bartowski    the episode / quality
      FILE   SSD1  models/qwen3-coder-q4.gguf        a copy (instance)
      FILE   drawer  backup/qwen-q4.gguf             another copy, same bytes
```

These names are closed. Do not invent a parallel "artefact" vs "instance"
vocabulary.

| Concept | Meaning | Identity |
|---|---|---|
| **Storage root** | A directory the user registers. Janus owns the *list*, not the files (except the fetch root). | `mount_id` (volume UUID / serial) + path |
| **Blob** | The bytes. Two files with the same blob are the same content. | BLAKE3 + size; SHA-256 stored alongside |
| **File (instance)** | One path on one root. Present or missing with the root. This *is* the copy. | `(root_id, rel_path)` |
| **Role** | Why this file belongs to a model: `weights`, `shard`, `tokenizer`, `config`, `mmproj`, `lora`, `sidecar`. | on the file ↔ variant (or family) link |
| **Variant** | A distinct representation people collect. | family + revision + format + quant + subflavour + **publisher** |
| **Revision** | A version: HF commit/tag, or synthetic `local:<blake3-prefix>` when unknown. | per family |
| **Family** | The logical model. | `family_key` (see §8) |
| **Companion** | Tokenizer / mmproj / projector / chat template sitting next to weights. Same family (or variant), different role — not their own family unless unidentified. |
| **Provenance** | Immutable event: downloaded, copied, seen-in-Ollama, imported, user-named. | attached to file, variant, or family |
| **Enrichment** | External payload, stored separately, level=`external`. | never merged into `known` |
| **Monitor** | User intent: watch this family (or variant) against a quality profile. | radar input |
| **Wanted item** | A remote file that matches a monitor/profile and is not satisfied locally. | radar output; fetch input |
| **Job** | Persisted background work (scan, hash, radar, fetch). Resumes across restarts. | |

**Publisher is a variant axis, not a filename stopword.** bartowski vs
mradermacher vs official vs local `convert_hf_to_gguf` are different Q4_K_M
files. People collect them on purpose. Ignoring `bartowski-` is how you
false-merge and hide the thing hoarders sort on.

**Sharded / dir models** are one variant with many files (roles `shard` /
`config` / `tokenizer`), grouped by directory signature (`index.json` +
shards, diffusers `model_index.json`, GGUF split `*-00001-of-00003`). This is
first-class, not an afterthought.

### Truth levels

Every stored *fact* (a field value) carries one level. The UI shows it.
Inferred is never presented as known.

| Level | Meaning | Example |
|---|---|---|
| `known` | From authoritative content or the user | GGUF KV arch, user-typed name |
| `detected` | From magic / structure | "GGUF v3, 96 tensors" |
| `inferred` | Filename heuristics / grouping | quant from `Q5_K_M` in the name |
| `external` | HF / gguf-index / radar listing | "HF says this repo is …" |
| `manual` | User override | merge, rename, "this is the same family" |

Implementation: an `evidence` table, not a single `identity_confidence` on the
family row. Home-page counts that are mostly inferred must say so
("128 families, 41 inferred").

---

## 5. Metadata schema

SQLite, WAL, one file, FTS5. Types omitted below are the obvious ones.
Schema version lives in `meta(k,v)`; migrations are mandatory from day one.

```sql
meta(k TEXT PRIMARY KEY, v TEXT);          -- schema_version, etc.

storage_roots(
  id INTEGER PRIMARY KEY,
  name TEXT,
  path TEXT UNIQUE,                        -- canonical absolute
  kind TEXT,                               -- internal | nas | removable | discovery | fetch
  mode TEXT DEFAULT 'catalogue',           -- catalogue | fetch
  mount_id TEXT,                           -- volume UUID / serial; NOT st_dev
  present INTEGER,
  last_present_check INTEGER,
  last_scan_at INTEGER,
  cold INTEGER DEFAULT 0,
  writable INTEGER DEFAULT 0               -- 1 only for fetch root
);

blobs(
  id INTEGER PRIMARY KEY,
  blake3 TEXT UNIQUE,
  sha256 TEXT,                             -- filled in the same hash pass
  size INTEGER NOT NULL,
  refcount INTEGER,
  xxhash64_partial TEXT                    -- first+last 64KiB; cheap gate
);

files(
  id INTEGER PRIMARY KEY,
  root_id INTEGER REFERENCES storage_roots,
  rel_path TEXT,
  size INTEGER, mtime INTEGER, ctime INTEGER,
  dev INTEGER, ino INTEGER,                -- hardlink identity
  is_symlink INTEGER, symlink_target TEXT,
  blob_id INTEGER REFERENCES blobs,
  hash_state TEXT DEFAULT 'none',          -- none|partial|full
  -- presence is inherited from the root; do not stamp missing on every row
  parse_state TEXT DEFAULT 'pending',      -- pending|ok|unsupported|partial
  parse_error TEXT,
  UNIQUE(root_id, rel_path)
);

model_families(
  id INTEGER PRIMARY KEY,
  family_key TEXT UNIQUE,
  name TEXT,
  arch TEXT,
  params_total REAL,                       -- not INTEGER; 30.5, MoE totals
  params_active REAL,                      -- nullable; MoE
  context_len INTEGER,
  kind TEXT DEFAULT 'unknown'              -- llm|vision|audio|embeddings|rerank|adapter|diffusion|unknown
);

family_aliases(
  family_id INTEGER REFERENCES model_families,
  alias TEXT,
  source TEXT,                             -- user|inferred|external
  UNIQUE(alias)
);

model_revisions(
  id INTEGER PRIMARY KEY,
  family_id INTEGER REFERENCES model_families,
  rev_kind TEXT,                           -- commit|tag|local
  rev_label TEXT,
  source_hint TEXT
);

model_variants(
  id INTEGER PRIMARY KEY,
  family_id INTEGER REFERENCES model_families,
  revision_id INTEGER REFERENCES model_revisions,
  quant TEXT,                              -- normalised
  quant_raw TEXT,                          -- GGUF file_type or filename tag
  format TEXT,                             -- gguf|safetensors|onnx|mlx|…
  subflavour TEXT,                         -- base|instruct|chat|thinking|coder|finetune|merge|lora
  publisher TEXT,                          -- bartowski|official|local|…
  UNIQUE(family_id, revision_id, format, quant, subflavour, publisher)
);

file_roles(
  file_id INTEGER REFERENCES files,
  variant_id INTEGER REFERENCES model_variants,   -- nullable if family-only
  family_id INTEGER REFERENCES model_families,
  role TEXT,                               -- weights|shard|tokenizer|config|mmproj|lora|sidecar
  PRIMARY KEY (file_id)
);

evidence(
  id INTEGER PRIMARY KEY,
  subject_type TEXT,                       -- family|revision|variant|file|blob
  subject_id INTEGER,
  field TEXT,
  value TEXT,
  level TEXT,                              -- known|detected|inferred|external|manual
  source TEXT,
  recorded_at INTEGER
);

provenance_entries(
  id INTEGER PRIMARY KEY,
  subject_type TEXT, subject_id INTEGER,
  event TEXT,                              -- downloaded_from|copied|seen_in|imported|user_named
  source_kind TEXT,                        -- hf|url|path|ollama|lmstudio|hf_cache|manual
  url TEXT, repo TEXT, author TEXT, licence TEXT,
  revision TEXT, at INTEGER, checksum TEXT
);

enrichments(
  id INTEGER PRIMARY KEY,
  subject_type TEXT, subject_id INTEGER,
  provider TEXT,                           -- hf|gguf-index|modelscope|…
  payload_json TEXT,
  fetched_at INTEGER, etag TEXT
);

quality_profiles(
  id INTEGER PRIMARY KEY,
  name TEXT UNIQUE,
  spec_json TEXT                           -- see §11
);

monitors(
  id INTEGER PRIMARY KEY,
  family_id INTEGER REFERENCES model_families,
  variant_id INTEGER,                      -- nullable = whole family
  profile_id INTEGER REFERENCES quality_profiles,
  enabled INTEGER DEFAULT 1
);

wanted_items(
  id INTEGER PRIMARY KEY,
  monitor_id INTEGER REFERENCES monitors,
  provider TEXT,
  repo TEXT, revision TEXT, filename TEXT,
  size INTEGER, sha256 TEXT,
  status TEXT,                             -- open|satisfied|fetching|fetched|skipped_have_bytes|dismissed
  local_blob_id INTEGER,                   -- set when catalogue already has sha256
  local_root_id INTEGER                    -- where those bytes live (may be offline)
);

fetch_tasks(
  id INTEGER PRIMARY KEY,
  wanted_id INTEGER REFERENCES wanted_items,
  dest_root_id INTEGER REFERENCES storage_roots,
  dest_rel_path TEXT,
  bytes_done INTEGER, bytes_total INTEGER,
  state TEXT,                              -- queued|running|paused|done|error
  error TEXT
);

tags(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
tagmap(tag_id INTEGER, entity_type TEXT, entity_id INTEGER,
       PRIMARY KEY (tag_id, entity_type, entity_id));

jobs(id INTEGER PRIMARY KEY, kind TEXT, state TEXT, progress REAL,
     started INTEGER, finished INTEGER, error_json TEXT);
scan_runs(id INTEGER PRIMARY KEY, root_id INTEGER,
          started INTEGER, finished INTEGER,
          files_new INTEGER, files_changed INTEGER, files_gone INTEGER, ok INTEGER);

-- FTS5 over name + aliases + arch + kind + tags + repo; maintained in the writer.
```

Indexes: `(root_id)`, `(blob_id)`, `(blake3)`, `(sha256)`, `(family_id)`,
`(kind)`, `(size)`, `(params_total)`. Filtering is SQL; search is FTS5.

User aliases, declined merges, and manual names are the irreplaceable rows.
`export` must include them. A file-list dump is not a backup.

---

## 6. Filesystem / storage

**Janus state (platform dirs, not hardcoded `~/.local/share`):**

```
$data/janus/          janus.db, logs/
$cache/janus/         http/ (enrichment + radar listings), tmp/ (hash + fetch parts)
$config/janus/        config.toml
```

Linux XDG, macOS Application Support, Windows LocalAppData — via a platform
dirs crate from day one.

**Root kinds**

| Kind | Mode | Janus writes? | Role |
|---|---|---|---|
| User folder, NAS, removable | `catalogue` | Never | The hoard. Default. |
| Ollama / LM Studio / HF cache | `catalogue` + `discovery` | Never | Smarter parser, report-only forever |
| Fetch root (exactly one default) | `fetch` | Yes, only here | Where radar-approved downloads land |

`janus root add ~/models` → catalogue, read-only.
`janus root add --kind fetch ~/models/inbound` → the only place fetch may
write. Sonarr's split: library vs download destination. Downloads never land
inside a random catalogue root.

**Catalogue mode is the default and the trust contract.** Remove Janus and
user folders are untouched. No `.janus-root` id file unless the user opts in
(needed only when volume UUID is unavailable).

**Managed reorg** (hardlink-into-pretty-layout, materialise, undo journal) is
specified as *later*. It is not how you use Janus. Do not leak it into v1.

**Root identity.** `mount_id` = filesystem UUID (Linux), volume serial
(Windows), Disk Arbitration UUID (macOS). `st_dev` is **not** the id — it
changes across remounts, USB ports, and containers. Path is a hint; `mount_id`
is the root.

**Presence is a property of the root.** When a root is gone: `present=0`.
Do **not** update every file row to `missing` (write storm, dirty diffs).
Files inherit presence. Hysteresis: N consecutive probe failures before
`present=0` so a NFS blip is not a library funeral. `cold` roots are not
polled; rescan on explicit mount or `janus scan`.

**Reconcile on return:** match `(rel_path, size, mtime, ino)` then reattach
blob. History is append-only (`scan_runs`); a scan does not delete provenance.

**Symlinks / hardlinks.** Symlink: store target, do not follow out of the
root. Hardlinks: same `(dev,ino)` → same allocation; reclaimable math uses
unique inodes per blob, not `size × (N−1)` file rows. Ollama blob hardlinks
are detected this way — no double-counting disk.

**Partials.** `.part`, `.part_file`, `.aria2`, `.!qB`, `.crdownload`, and
fetch staging files → `parse_state='partial'`. Shown as incomplete, never as
models. Fetch uses `$cache/janus/tmp/fetch/` then atomic rename into the fetch
root.

**Windows cloud placeholders (OneDrive / iCloud).** Skip unhydrated reparse
points. A naïve walk that hydrates 4 TB is a product defect, not a Low risk.

---

## 7. Hashing and deduplication

### One pipeline, one streaming pass

Full-file BLAKE3 *and* a later SHA-256 for gguf-index is how first-scan dies
on a NAS. Compute both in one read.

1. **Walk gate** — `(size, mtime, ino)` vs last scan. Unchanged → skip.
2. **Header parse** — identity without reading weights (mmap / bounded read).
3. **Partial gate** — xxHash64 of first+last 64 KiB when size collides with
   an existing blob. Cheap reject.
4. **Full pass** — stream once → BLAKE3 + SHA-256 onto `blobs`.
5. **Reuse known digests** — Ollama blob names are `sha256-…`; HF cache has
   LFS sha256. After a size check, trust them; do not rehash the Ollama store
   on first scan.
6. **Resume** — `jobs` per file, not only per root.

`--quick` = walk + header parse, no full hash. Enough to group and browse.
Duplicates and gguf-index identify need the full pass (or a reused digest).

First-scan I/O on 4–10 TB is a **High** risk. Background, I/O-nice, per-root
jobs, user-visible "hashed 12 / 400 files (800 GB left)".

### Four kinds of "duplicate" — never conflate

1. **Byte-exact** — same BLAKE3. Certain. Ollama ↔ LM Studio ↔ HF cache copies
   are the common case.
2. **Same bytes, different path** — one blob, N files. Reclaimable = unique
   extra *inodes*, not extra rows.
3. **Same model, different representation** — variants. Never dedup.
4. **Near-dup** (re-packed weights) — post-v1 tensor signatures. Suggest only.

### Reclaim is report-only in v1

The persuasive number is `reclaimable`. Show it. Do not delete or hardlink
until later, and **never** mutate discovery roots (Ollama, LM Studio, HF
cache) — those apps own their refcounts.

When apply exists: refuse if any surviving copy's root is offline; use trash
not unlink; journal is not a restore of bytes. Cross-volume hardlink is
impossible; the button must not pretend otherwise.

---

## 8. Model identification

**Detect by content, never by extension.**

| Format | Detection | Extract |
|---|---|---|
| GGUF | magic `GGUF` | KV name/basename/finetune/file_type, arch, blocks, heads, params, context, tensors |
| SafeTensors | LE header length + JSON | architectures, quant, param count from shapes. Bounded header; no full-file read |
| ONNX | protobuf + graph | producer, ir, I/O, opset — shallow |
| MLX / quantized ST | safetensors + `quantization_config` | as safetensors + quant tags |
| PyTorch `.bin/.pt/.pth` | **never unpickle** | adjacency + `config.json` only. No sandbox "later" |
| LoRA | adapter naming / GGUF `*.lora` | role=`lora`, base hints |
| Tokenizers | names + shapes | role=`tokenizer` on a family |
| Diffusers dir | `model_index.json` | kind=`diffusion`, v1 = structural only |
| Unknown | anything else | first-class Unknown; hashed; searchable |

Parser DoS: cap safetensors header size, cap GGUF KV, never mmap device/sparse
traps, never follow symlinks out of the root.

**Filename heuristics** are `inferred` only: quant tags, params tags,
subflavour, publisher (kept, not stripped). Content wins; filenames fill gaps.

### `family_key` (pure function)

The three files in the brief must become one family:

```
qwen3-coder-q4.gguf
Qwen3-Coder-30B-Q5_K_M.gguf
Qwen3-Coder-30B-Q8.gguf
```

Algorithm (golden-tested; changing it is a migration):

1. Read GGUF KV (`known`): `general.basename`, `general.name`,
   `general.finetune`, architecture, block_count, head_count, tensor-derived
   params.
2. Parse filename (`inferred`): tokens for name, params, quant, subflavour,
   publisher.
3. **Display name** prefers basename, then name, then filename stem with
   quant/publisher tokens stripped. Params class from tensors only as a
   *guard rail* (MoE / vision towers must not invent "30B" blindly).
4. `family_key = slug(display_name) + "|" + arch + "|" + params_bucket`
   where `params_bucket` is a coarse bin (`7-8B`, `27-35B`, `unknown`) so
   Qwen3-30B-A3B and Qwen3-32B do not collapse, and Q4/Q5/Q8 of the same
   basename do.
5. Variant = `(format, quant, subflavour, publisher)` under a revision
   (`hf:…@sha` if known, else `local:<blake3-8>`).
6. **No silent merges.** Same `family_key` from `known` fields may attach.
   Anything that only agrees on inferred tokens goes to a merge-suggestion
   queue. User merge/rename writes `family_aliases` and is forever respected.

Hard cases that must have fixtures before this ships:

- bartowski vs official vs local convert of the "same" Q4_K_M
- instruct vs base vs thinking vs coder
- Qwen3-30B-A3B vs Qwen3-32B
- sharded safetensors + `index.json`
- split GGUF; mmproj beside the LLM
- HF cache snapshot vs a renamed copy in `~/models` (same blob, rich vs poor
  provenance)
- empty-metadata `random.safetensors`

**Unknown is first-class.** Hashed, browsable, searchable before it has a
name. Identify = reparse → hashes → gguf-index / HF by sha256 → user may type
a name (`manual` provenance).

External metadata confirms and fills licence/author/repo. It never overrides
local `known` fields.

---

## 9. External providers

Four plugin shapes. Same network stack (reqwest, timeouts, ETag cache under
`$cache/janus/http`). Nothing is required for the catalogue to work.

| Trait | Job | MVP |
|---|---|---|
| **DiscoverySource** | Where to look on disk | User paths; Ollama manifests (`OLLAMA_MODELS` or `~/.ollama/models`); LM Studio models dir; HF hub cache (`models--*` snapshots) |
| **EnrichmentProvider** | Facts about a known id | Off by default. HF model card; gguf-index (local parquet/sqlite of sha256→repo/file, not a live Mozilla API) |
| **AvailabilityProvider** | Remote file lists for radar | HF repo tree (`?revision=`) with size + sha256. ModelScope later |
| **AcquisitionProvider** | Get those bytes | HTTPS resume into fetch staging. Gated HF via optional token |

**Guarantees**

- No write to upstream. No account required for catalogue or radar of public
  repos.
- Every cached payload has `provider / fetched_at / etag` and survives the
  source dying.
- "HF is down" is never "your library broke."
- **Privacy:** `scan` sends nothing. Radar and identify-via-index send
  identifiers the user asked for (repo id, or sha256 of a file they chose to
  look up). Hashing a local hoard to a public index fingerprints it — say so
  in the UI, per provider, opt-in. Filenames of local files are not uploaded
  unless the user is searching a provider on purpose.

---

## 10. Offline / removable storage

The hoarder case (SSD1 + SSD2 + NAS + drawer) is a design input, not an edge.

- Each root is identified by `mount_id`, not the current path.
- Probe on startup and on a timer (stat + read a few bytes). Cold roots: no
  poll.
- Mount events best-effort (udisks2, DiskArbitration, WM_DEVICECHANGE);
  fallback is poll + `janus root probe`.
- A family with copies on four drives is **one family, four files**. Offline
  drawer → family still browsable: "present on SSD1; drawer-drive last seen
  Tuesday."
- Search and radar treat offline copies as **owned**. Reveal-in-file-manager
  is the only thing that requires `present=1`.
- Watch (`notify`) on a 15 TB NFS export is unreliable. v1 is poll + explicit
  scan + mount events. A live watcher is later, optional.

Mental model: **immutable history + reconcilable present.**

---

## 11. Radar

Radar is Sonarr's Wanted tab, not a trending-models homepage.

It answers, for families you care about: *what remote files exist, which of
those do I already have (anywhere, including offline), which match my
profile, which are genuinely missing?*

### Quality profile

```toml
# $config/janus/config.toml  (also editable in UI)
[[profiles]]
name = "daily-llm"
formats = ["gguf"]
quants = ["Q4_K_M", "Q5_K_M"]          # acceptable
cutoff = "Q4_K_M"                      # stop wanting once this exists
publishers = ["bartowski", "official"] # preference order; empty = any
max_bytes = "40GB"
exclude_name = ["i1", "-IQ"]           # inferred filename tokens
```

Cutoff is the *arr idea: have Q3 → still wanted; have Q4_K_M → satisfied even
if Q8 exists, unless the user raises the cutoff.

### Monitor

A monitor is `(family | variant) + profile`. You can monitor a family you
already own ("tell me when a new revision or a missing acceptable quant
appears") or pin a specific hole ("this family's Q5_K_M").

You do not have to monitor everything. Unmonitored families still catalogue
and still show a manual "what's on HF" action.

### Sweep

For each enabled monitor:

1. Resolve a remote identity (HF repo from provenance / enrichment / user).
2. AvailabilityProvider lists files (name, size, sha256) — cached, ETag.
3. For each remote file that matches the profile:
   - If `sha256` matches a local blob → `skipped_have_bytes`, record which
     root (may be offline). **This is the Janus rule: never fetch bytes the
     catalogue already has.**
   - Else if a local variant already meets cutoff (same format/quant/publisher
     rules, even if sha256 differs — different converter) → satisfied, unless
     the user asked for that exact publisher.
   - Else → `wanted_items` row, status `open`.

Radar is read-only. It does not download. It may run on a timer in the daemon
or via `janus radar`.

### What radar is not

- Not "trending on HF."
- Not a search portal that replaces huggingface.co.
- Not auto-grab (that is a later flag on the monitor).
- Not an indexer/torrent network.

---

## 12. Fetch

Fetch is Radarr grabbing *one release you already decided you want*.

```
wanted item (open) → confirm → fetch_task → $cache/tmp/fetch/*.part
  → checksum (sha256 from listing) → atomic rename into fetch root
  → same ingest as scan (parse, hash, group, provenance=downloaded_from)
```

**Rules**

1. Destination is the fetch root only. No writes to catalogue or discovery
   roots.
2. Refuse if a blob with that sha256 already exists, including offline —
   surface "owned on drawer-drive" and require `--force` to duplicate.
3. Resume via HTTP Range; `.part` files are `partial` in the catalogue if
   they are visible, but staging stays in cache until verify passes.
4. Gated repos: optional `HF_TOKEN` from env / config. Catalogue never needs
   it.
5. One writer owns fetch_tasks (the daemon if running). CLI `janus fetch`
   either calls the daemon or takes the write lock.
6. After success the file is a normal catalogue row. Radar marks the wanted
   item `fetched`. There is no second "import" step.

**What fetch is not**

- Not a parallel-connection download appliance (aria2/modfetch exist; a later
  hook may shell out).
- Not "download into Ollama." Runtimes remain consumers of paths Janus can
  print.
- Not the home screen, not the first-run wizard, not how families are
  discovered.
- Not BitTorrent, not usenet, not a remote agent farm.

Auto-fetch on radar hit is post-v1 (`monitors.auto_fetch`, default off).

---

## 13. CLI

Single binary `janus`. Human tables by default, `--json` everywhere.

```bash
# roots
janus root add PATH [--name] [--kind nas|removable|internal|discovery|fetch] [--cold]
janus root ls | rm ID | probe ID
janus root doctor

# catalogue
janus scan [ROOT...] [--quick] [--no-hash]
janus list [--kind] [--family] [--root] [--offline] [--dups] [--quants-of FAM]
janus search QUERY [--quant] [--min-params] [--max-params] [--offline] [--source]
janus show <id|path>
janus quants <family>                  # local ladder; + radar holes if enriched

# identity
janus identify FILE [--deep] [--non-interactive]
janus enrich [--model X | --all] [--provider hf|gguf-index] [--dry-run]
janus merge SRC TARGET
janus verify <target> [--full]

# radar + fetch  (outward face)
janus profile ls|show|set
janus monitor add FAMILY [--profile daily-llm] [--auto-fetch]   # auto-fetch off
janus monitor ls | rm ID
janus radar [FAMILY...] [--once]       # sweep; write wanted_items; no download
janus wanted [--open|--have-offline]
janus fetch WANTED_ID|REPO --file NAME [--force]   # into fetch root
janus fetch status | pause | resume

# storage
janus dedup [--plan]                   # report-only in v1; no --apply yet
janus storage [summary|tree|dups|roots]
janus cold mark|unmark ID

# process
janus daemon [--api 127.0.0.1:4321]    # writer + API + UI; optional
janus status | doctor | export | import | completions
```

`scan` is how existing files enter. `fetch` is how new remote files enter.
There is no `import` of a random path that isn't a root, and no `/ingest`
API that is a disguised downloader.

`list` / `search` / `show` / `wanted` share one query engine with the UI.

---

## 14. UI

Local web UI, bundled, served by `janus daemon` at `http://127.0.0.1:4321`.
Cross-platform without Electron. Tauri (tray, native "reveal in Finder") is
later. The web app stays the interface.

```
Home         counts with truth split; TB; reclaimable (report); unknown;
             offline roots; open wanted (if any). Not a download queue.
Library      families; filters; variant ladder as the hero.
Model        identity + truth badges; instances per root; provenance;
             "radar this family"; missing-vs-profile chips; fetch button
             only on an open wanted item.
Radar        monitored families; last sweep; open / have-offline / fetched.
             Confirm fetch. No trending grid.
Wanted       the *arr tab. Offline-owns-it is a first-class row, not missing.
Storage      treemap by root → family; dup report.
Unknown      identify inbox.
Search       Cmd/Ctrl-K; chips: quant:, params:, offline, wanted, have-bytes.
```

Dense tables, dark-first, zero signup. No `likeCount`, agent scores, or
pipeline graphs.

Fetch progress is a job row (poll or SSE). The API is

`/api/v1/{roots,models,files,storage,dups,search,jobs,radar,wanted,fetch}`

localhost only by default.

---

## 15. Technical architecture

**Language: Rust.** One static binary; streaming hash; untrusted header
parse; later-optional Tauri. Go would not change the architecture.

**Data: SQLite WAL.** Personal scale (thousands of models, tens of thousands
to low hundreds of thousands of files). No Postgres, Redis, or S3.

**Libraries:** clap · rusqlite (bundled FTS5) · tokio · walkdir · blake3 ·
xxhash-rust · sha2 · memmap2 · serde · axum · reqwest · rust-embed.
Frontend: Svelte 5, compiled at Janus build time, embedded. There *is* a
frontend build; the user does not run it.

**Process / writer (pick one, this is it):**

- If `janus daemon` is running, it is the only SQLite writer. CLI is a client
  (unix socket or localhost API).
- If it is not running, CLI takes an advisory lock and writes, then exits.
- Never two hashers plus a UI plus a CLI all writing.

Workers: I/O pool (hash/parse/fetch). One writer task, batched transactions.
WAL readers (UI, `janus list`) stay live. A file that fails to parse is
`unsupported` with an error — the run does not die.

**Watch** is not a v1 requirement. Daemon in v1 exists for long hash/radar/
fetch jobs + UI. `janus scan` without a daemon is a supported way to live.

**No containers as a design goal.** A NAS Docker recipe can live in the
README later.

---

## 16. Phased scope

The inward face ships first. Radar without fetch is already useful. Fetch is
designed now so it does not land as a bolted queue later.

### Phase A — catalogue (smallest useful Janus)

1. `root add/ls/probe`, catalogue-mode scan.
2. Detect + parse: GGUF, safetensors, ONNX (graph I/O), companion
   `config.json`; everything else structural / Unknown.
3. Hash pipeline §7; hardlink/symlink aware; **dedup report-only**.
4. Conservative family/variant grouping; merge suggestions; no silent merge.
5. Offline roots (inherited presence, hysteresis, cold).
6. FTS5 + filters; CLI `list/search/show/storage`.
7. Unknown + `identify` (local; optional sha256 lookup opt-in).
8. Provenance for observed files; discovery sources read-only.
9. Export/import of db essentials (roots, blobs, aliases, user decisions).

### Phase B — daemon + UI

Library, model page, storage treemap, unknown inbox, truth badges.
Same query engine as the CLI.

### Phase C — radar

Profiles, monitors, `janus radar` / Radar UI, wanted list, have-offline.
HF availability listings + cached gguf-index. No download.

### Phase D — fetch

`janus fetch` of an open wanted item (or an explicit repo+file) into the
fetch root; resume; verify; ingest; refuse if bytes already owned.

### Explicitly later

Managed reorg, near-dup tensor signatures, auto-fetch, more providers
(ModelScope, Civitai), update-vs-`main` badges as a nicety on top of radar,
Tauri, collections/smart views, VRAM-fit guessing, launch-existing-runtime
shortcuts, download-client hook (aria2), community family-key maps.

### Explicitly never (read every roadmap cycle)

- Inference, generation, chat, OpenAI-compatible serving, routers.
- Hub upload / publishing.
- Conversion, quantization, training, experiment tracking.
- Multi-user, RBAC, cloud sync, S3, Kubernetes, microservices.
- A default managed cache Janus downloads into so it can "own" the library.
- Anything that requires the internet for the catalogue to function.
- Mutating Ollama / LM Studio / HF-cache files.
- Unpickling `.bin/.pt/.pth`. No sandbox exception.
- Trending / social / "recommended for you" as the home screen.
- BitTorrent, usenet, or a Servarr indexer ecosystem.

---

## 17. Post-phase roadmap

1. Auto-fetch flag on monitors (default off); cutoff upgrades.
2. More Availability/Acquisition providers; community `family_key` maps
   (the contribution surface).
3. Near-duplicate / re-packed grouping.
4. Read-only API for llama-swap / agents to *query* the catalogue.
5. Tauri, collections, VRAM-fit, "open in LM Studio / llama.cpp" as a path
   handoff.
6. Cold-storage rehash-on-mount ("verified" through offline periods).
7. Optional aria2/modfetch as an AcquisitionProvider backend.

Every later feature attaches to identity + provenance + wanted items.
Nothing new gets its own parallel library.

---

## 18. Risks

| Risk | Sev | Mitigation |
|---|---|---|
| Filename / Jaccard false merges | High | `known`-first; no silent merge; aliases; fixtures for hard cases |
| Sharded / companion grouping | High | Dir detector + `file_roles`; mmproj is a role, not a family |
| First-scan hash cost | High | One-pass BLAKE3+SHA256; digest reuse; `--quick`; resumable jobs |
| Fetch when bytes already exist offline | High | sha256 match against all blobs; wanted status `skipped_have_bytes` |
| Fetch writes the wrong place / breaks Ollama | High | Single fetch root; discovery roots never writable |
| Cloud-placeholder hydration | High | Skip unhydrated reparse points |
| Pickle RCE | High | Never unpickle |
| External facts laundered as known | Med | `evidence` + separate `enrichments` |
| Flaky NAS | Med | Inherited presence, hysteresis, cold |
| Privacy leak via hash lookup | Med | Opt-in providers; scan is offline; UI names what leaves |
| Scope creep to "another modfetch" | Med | Home screen is the catalogue; §16 never-list |
| Windows case / ADS / reparse | Med | Canonical keys; volume serial; test two OSes |
| `family_key` churn | Med | Golden tests; schema_version; aliases survive |
| Parser bombs | Med | Header caps; no out-of-root follow |

---

## 19. Design questions

**How do the three Qwen files become one model?**
Content-first GGUF KV → same `family_key`. Variants differ by quant.
Publisher preserved. If `known` fields disagree, two families and a merge
suggestion — never forced.

**Unknown models?**
Unknown inbox; hashed and searchable before naming.

**File ownership?**
Catalogue-in-place by default. One fetch root is the only tree Janus writes.
No forced reorg.

**Same 20 GB on four drives?**
One variant, four files, one blob. Library shows one family with four
locations and presence. Dedup reports reclaimable inodes. Radar treats it as
owned.

**Offline drive?**
Root `present=0`; files inherit; family and radar still see the blob.
Reconnect reconciles. Fetch will not quietly re-download.

**Variants?**
format × quant × subflavour × publisher × revision. Roles (mmproj, tokenizer)
hang off the variant/family, not a fifth "kind of model."

**What's eating 8 TB?**
Storage view: root → family; blob-unique sizes so hardlinked copies do not
triple-count.

**What's missing, and can I get it?**
Radar diffs a profile against remote listings and local blobs. Fetch pulls
one open wanted item into the fetch root and catalogues it. That is the
whole outward face.

---

## 20. Recommendation

Build the inward face until a messy multi-TB scan produces: families (with
honest inferred counts), a reclaimable number, an Unknown inbox that can
become named, and grey "in the drawer" states. That is the product people
do not have.

Build radar next so the same screen can say "you have Q4; Q5 is on HF; Q8 is
already on the unplugged NAS." Build fetch last, as a boring verified copy
into one folder — Sonarr, not Chrome.

The trap is not "adding fetch." The trap is letting fetch become the app.
Janus stays a catalogue that can acquire. It does not become an acquirer that
also lists files.

**Crate order**

`janus-core` (scan / hash / parse / identity / db) → `janus` CLI → daemon +
UI → `janus-radar` → `janus-fetch`.

**Milestones:** (A) scan+hash+group+CLI, (B) UI, (C) radar/wanted,
(D) fetch. Everything else is roadmap.
