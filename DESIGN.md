# Janus — Architecture & Product Design

**Working title:** Janus — the Plex/Jellyfin of your local AI model hoard.

**One-line definition:** A local-first, catalogue-first library for the AI model
artefacts you already own — it scans your drives, figures out what things are,
groups them into families/revisions/variants, deduplicates, tracks provenance,
survives offline drives, and gives you one beautiful place to answer *"what the
hell do I actually have?"*

This document is opinionated. Where there are trade-offs, I pick a side and
say why.

---

## 1. Product definition

Janus is **a catalogue, not a runtime**. It is the librarian for a personal
collection of AI model files, regardless of format, origin, or runtime.

**It does for model files what Plex does for media:**

| Plex / Jellyfin | Janus |
|---|---|
| You own a pile of `.mkv`/`.mp4` in random folders | You own a pile of `.gguf`/`.safetensors`/`.bin` in random folders |
| Media manager *identifies* titles and season/episode | Model manager *identifies* family/revision/variant |
| Matches against online metadata (TMDB), marked "matched" vs "unmatched" | Matches against local metadata + optional HF enrichment, marked with confidence |
| Handles libraries on removable drives going offline | Same |
| Listens to filesystem, rescans, shows "newly added" | Same |
| Does not transcode unless you ask it to | Does not run inference unless you (later) ask it to |

**Target user:** a technically sophisticated person who accumulates terabytes
of AI models across drives and tools, and wants to finally *see* the collection.

**Core promise:** "Figure out what all this stuff is" — without moving a single
file, without an account, without the cloud, and without lying to you about
what is known vs guessed.

---

## 2. Existing-project landscape

I researched the current ecosystem (Aug 2026). The space splits into four
archetypes, plus one "not-even-close" category.

### A. Runtimes & downloaders that own their models
These *download* into their own structured store and manage a library of *what
they downloaded*. They cannot catalogue a pre-existing 15 TB scatter.

- **Ollama, LM Studio, Jan, GPT4All** — runtime + per-app cache. Own opinions
  about layout; not viewers of arbitrary collections.
- **lilbee, llamaUI, hfdesk, EZmodL, BC ModelVault, modfetch, modeldock,
  ggufy, modelflow-class downloaders** — download/manage/serve GGUF or HF
  models, with queues, VRAM fitting, `llama-swap.yaml`, profiles, verify.
  Download-first, runtime-flavoured.
- **model-shelf (several projects), modelshelf (Rust SDK/CLI)** — the most
  interesting neighbours. `modelshelf` is a *shared local model registry*:
  per-machine JSON registry + content-addressed blob store + hardlink dedup +
  HF update checks + recommendation, explicitly "no daemon." It is an
  interoperability SDK for desktop apps, not a browse-first catalogue of an
  existing messy hoard, and not a provenance/offline-drive system.

### B. Scanners & disk cleaners
- **ai-model-scanner** — find erratic models, duplicates, cleanup, watch.
  CLI, report-only, no persistent identity model, no UI.
- **dehoard** — macOS cleaner that finds the same LLM across Ollama/LM Studio/HF
  cache and, importantly, *distinguishes true duplicates from related variants*
  (Q4 ≠ Q8). Report-only.
- **DupeRangerAi / generic dup finders** — byte-hash dupes; model-blind.
- **Validator of the pattern:** **File Hunter** catalogues files across
  offline USB/backup drives, three-gate hashing (size → xxHash partial →
  xxHash full → optional SHA-256), SQLite WAL. This is *exactly* the storage
  substrate Janus needs — but it doesn't know what a quantization is.

### C. Self-hosted HF-Hub replacements
- **HuggingHack, KohakuHub, hf-local-hub, CSGHub, MatrixHub, mini-hf, oh-my-hf**
  — rehost/proxy/cache the Hub, generally with a web UI, accounts, S3.
  Org/enterprise or "private mirror" oriented. Not personal catalogue of
  existing junk.

### D. Adjacent-domain catalogues worth stealing UX from
- **MeshVault, Modelist, STLHub** (3D files) — local SQLite catalogues,
  index-folders-in-place, hash dedupe, web-capable, browser-first. Proven UX
  patterns for "my hoard, browsable."
- **Latent-Model-Organizer** — diffusion-specific (SDXL/Pony/Flux) safetensors
  organizer with sidecar pairing + atomic undosable moves. Its *safe move*
  machinery is reusable; its scope is too narrow for Janus.

### Verdict on reinvent-vs-reuse
Do **not** fork modelshelf/ai-model-scanner. Do **reuse the ideas** of
content-addressing, three-gate hashing, "true dup vs variant" separation, and
on-disk registries. Janus's differentiators that no one covers together:

1. **Model-aware identity** (family/revision/variant/artefact) on *arbitrary
   existing folders* — no one owns this.
2. **Non-destructive catalogue mode** for root folders you don't reorganise.
3. **Multi-drive instance tracking + offline/removable storage** (the hoarder
   case: SSD1 + SSD2 + NAS + drawer).
4. **Provenance discipline** (source/URL/repo/date/checksum/labelled confidence).
5. **Browsing UI that answers "what do I have / what's eating my 8 TB /
   what quants am I missing"** — not a download queue, not a serve-all router.

**Gap statement:** the ecosystem has *downloaders*, *scanners*, and *hub
mirrors*. It lacks a *catalogue of the present*.

---

## 3. What gap Janus fills

See §2 verdict. Concretely, the moment someone says *"Qwen just released a new
model — cool, downloading"* and six months later has 4 TB, the existing tools
answer:
- "I have files here and here" (scanners)
- "download this, run that" (runtimes)
- "here's a private Hub" (mirrors)

**None answer: "what do I own, what is it, how much space, where, from where,
what's duplicated, what's missing, and is it on a drive that's currently in a
drawer?"** That is Janus. It is a first-class citizen of the *collector's*
lifecycle, orthogonal to whichever runtime they eventually run the files on.

---

## 4. Core concepts / domain model

Five layers of identity. Always ask "which layer is this fact attached to?"

```
MODEL FAMILY   Qwen3-Coder-30B                     (the "show")
  └─ REVISION  HF commit e3f2a1c / "local seed"     (the "season")
      └─ VARIANT  Q4_K_M · GGUF · Instruct          (the "episode")
          └─ ARTEFACT  file(s) on disk, provenance  (the "rendition")
          └─ ARTEFACT  same bytes copied on SSD2    (another rendition)
```

**Definitions**
- **Storage Root** — a directory the user registers (`root add ~/models`,
  a NAS mount, a removable drive); the only thing Janus "owns" is this list.
- **Physical Artefact (file)** — one file, one inode-path-history: root,
  rel_path, size, mtime, ctime, (dev,ino), symlink target, state, blob hash.
- **Blob** — content-addressed identity (BLAKE3 + size) + refcount. Two
  artefacts with equal blobs are the *same bytes*.
- **Model Family** — logical model identity: name, arch, params, context,
  kind (llm/vision/audio/embeddings/rerank/adapter/diffusion/unknown).
- **Revision** — a version of the model (HF commit, tag, or synthetic
  `local:<blob-prefix>` when unknown).
- **Variant** — quant, format (gguf/safetensors/onnx/mlx/…), subflavour
  (base/instruct/chat/finetune/merge/lora).
- **Instance** — one physical copy of an artefact; a model may have several
  instances (SSD1, NAS, drawer) — *one model, many locations*.
- **Provenance Entry** — immutable fact: `(artefact|variant, event)` where
  event ∈ {downloaded-from HF@rev, copied, imported, seen-in-Ollama, …} with
  URL, repo, author, licence, date, source-of-record, checksum.
- **Enrichment** — external metadata fetched on demand (HF model card,
  gguf-index hash lookup), stored **separately** and tagged as external.
- **Tags/Collections** — user organization, attaches to family or variant.
- **Job** — persisted background task (scan, hash, verify, enrich) that
  resumes across restarts.

**Truth levels** — every stored fact carries one of:

| Level | Meaning | Example |
|---|---|---|
| `known`    | read from authoritative content (GGUF KV, safetensors header) or user-set | arch = llama, params = 30.5B |
| `detected` | from format magic / structure | "this is GGUF v3, 96 tensors" |
| `inferred` | from filename heuristics / grouping | quant = Q5_K_M, family name |
| `external` | from HF/gguf-index/etc., never merged silently | "HF says this is Qwen/Qwen3-Coder" |

The UI must *show* these labels. Janus never presents `inferred` as `known`.

---

## 5. Metadata schema

SQLite, WAL, one file, FTS5. Draft DDL (the important nodes; rest follows).

```sql
storage_roots(
  id INTEGER PRIMARY KEY,
  name TEXT, path TEXT UNIQUE,              -- canonical absolute path
  kind TEXT,                                 -- internal | nas | removable | managed
  mode TEXT DEFAULT 'catalogue',             -- catalogue | managed
  mount_id TEXT,                             -- dev/label/uuid; survives renames
  present INTEGER, last_present_check, last_scan_at,
  cold INTEGER DEFAULT 0                       -- "this is deep storage"
);

blobs(id INTEGER PRIMARY KEY, hash BLAKE3 UNIQUE, size INTEGER, refcount,
      sha256 TEXT, sha256_state TEXT);         -- sha256 optional (enrich/verify)

physical_files(
  id INTEGER PRIMARY KEY, root_id REFERENCES storage_roots,
  rel_path TEXT, size INTEGER, mtime, ctime, dev, ino,
  is_symlink INTEGER, symlink_target TEXT,
  blob_id REFERENCES blobs, blob_state TEXT,   -- none|pending|full
  state TEXT DEFAULT 'present',                -- present|missing|partial|unsupported
  UNIQUE(root_id, rel_path)
);

model_families(
  id INTEGER PRIMARY KEY, name TEXT,
  family_key TEXT UNIQUE,                      -- normalized identity string
  arch TEXT, params_b INTEGER, context_len INTEGER,
  kind TEXT DEFAULT 'unknown',
  base_aliases TEXT,                           -- json: [qwen3-coder-30b, ...]
  name_confidence TEXT, identity_confidence TEXT
);

model_revisions(id, family_id, rev_kind, rev_label,  -- commit|tag|local
                params_b, context_len, source_hint);

model_variants(
  id, family_id, revision_id, quant, format, subflavour,
  num_params_eff, size_estimate, extra_json);

artefacts(
  id, variant_id NULL, file_id REFERENCES physical_files,
  role TEXT,                                    -- weights|tokenizer|config|mmproj|sidecar
  original_filename, imported_at);

provenance_entries(
  id, subject_type, subject_id, event TEXT,      -- downloaded_from|copied|seen_online|imported
  source_kind TEXT,                              -- hf|url|path|ollama|manual
  url, repo, author, licence, revision, downloaded_at, checksum TEXT);

enrichments(id, subject_type, subject_id, provider,               -- hf|gguf-index
            payload_json, fetched_at, etag, serialized_at NULL);  -- must be importable offline

tags(id, name) / tagmap(tag_id, entity_type, entity_id);
jobs(id, kind, state, progress, started, finished, error_json);
scan_runs(id, root_id, started, finished, files_new, files_changed, files_gone, ok);
```

Search: FTS5 table over `name + aliases + arch + kind + tags + repo`, kept in
sync from the write path. Filtering (quant, size range, params, offline,
duplicate, source, date) is plain indexed SQL — a few covering indexes
`(kind)`, `(family_id)`, `(blob_id)`, `(root_id, state)`, `(params_b)`,
`(size)`, plus the VIRTUAL column trick for range filters if needed.

---

## 6. Filesystem / storage design

**Janus's own state (never touches user files unless asked):**

```
~/.local/share/janus/
  janus.db            # SQLite WAL
  managed/            # only exists if user enables managed mode
  logs/
~/.cache/janus/
  http/               # enrichment cache (HF cards, gguf-index hits), offline-clean
  tmp/                # hash staging
~/.config/janus/config.toml
```

**Two modes, catalogue-first.**

1. **Catalogue mode (default, read-only).** `janus root add ~/models` → Janus
   reads metadata, hashes, indexes. It never creates files in user folders,
   never renames, never moves. The database is metadata-only. Remove Janus and
   your folders are exactly as you left them (the Modelist/File-Hunter promise).

2. **Managed mode (opt-in).** `janus root add --mode managed ~/library`
   imports copies via hardlink-when-possible (same filesystem) else copy, into
   an internally-organised space (per-family dirs with provenance sidecars +
   content-addressed fallback). Re-export / "materialize to a path" is
   supported. This is for people who *want* Janus to own a clean library, and
   for later "download into managed" flows. It is never the default.

**Never reorganise to catalogue.** If the user wants tidier on-disk structure,
provide `janus plan reorg` → writes a **dry-run manifest** (moves, shown as
preview) → apply with a journaled undo log (borrow Latent-Model-Organizer's
atomic-group + undo-manifest idea). Grouping "with their siblings" (sidecars,
previews, `.civitai.info`) must survive.

**Root identity for renames/offline.** Each root stores `mount_id`
(statvfs `st_dev` + filesystem label when obtainable, or a `.janus-root` id
file only if the user opts in). On scan, if the path is missing or I/O fails →
mark root `present=0` and all its files `state='missing'`; metadata stays.
When it reappears, rescan reconciles by `(rel_path, size, mtime, inode)` and
re-attaches by blob.

**Symlinks/hardlinks.** A symlink is catalogued as an artefact whose content
is the target path, with `state='symlink'` and `is_symlink=1`; standard
dedup marks hardlinked inodes (same dev+ino) as the same byte instance. Ollama
blob hardlinks are detected this way (see §9) — no double-counting.

**Partial/incomplete files.** Recognise `.part`, `.part_file`, `.aria2`,
`.!qB` and `.crdownload` suffixes → `state='partial'`; also flag files whose
size doesn't match the GGUF tensor-layout expectation or a known HF size when
enriched. Present them as "possibly incomplete" but never as models.

---

## 7. Deduplication strategy

Four distinct kinds — never conflate them:

1. **Byte-exact duplicates** — same BLAKE3 → same blob. Causes: copied twice,
   downloaded twice, tool caches copying each other (huge in practice:
   Ollama↔LM Studio↔HF cache). Handled automatically and *certain*.
2. **Same file, different name** — same blob, differing rel_path/source.
   Shown as one artefact with N instances; reclaimable = size×(N−1).
3. **Same model, different representation** — do NOT dedup; keep as variants
   (Q4_K_M vs Q8_0 vs safetensors) and *surface* them together.
4. **Near-duplicates** (converted/re-packed same weights, metadata-only diffs) —
   post-MVP "tensor-content signature": hash only the ordered tensor-type+
   shape+dims vector from GGUF/safetensors headers (not the bytes), plus
   sampled weight fingerprints. Compare cheaply, present as "likely merged/re-
   packed", never auto-remove.

**Pipeline (three gates, File-Hunter-proven):**
- Walking gate: `(size, mtime, inode)` — cheap; skips unchanged files.
- Candidates: xxhash3 fast pass over files above a size floor when quick-group
  needed; **canonical gate = BLAKE3 full-file**, stored on `blobs`.
- Strong gate on demand: SHA-256, for verify + gguf-index enrichment (§9).

**Reclaim logic is user-confirmed, journaled, undoable** — Janus *reports*
"these N files are the same bytes on root A/B/C; delete extras / hardlink one
store". Deletion/relink only via explicit `--apply`. `dedup` view shows
`reclaimable` up front — for a hoarder this is the single most persuasive
number in the product.

---

## 8. Model identification strategy

**Format detection by content, never by extension.** Magic bytes / structure:

| Format | Detection | Extracted metadata |
|---|---|---|
| GGUF | magic `GGUF` + version | `general.name/basename/finetune/file_type`(quant), arch (block count, heads…), params (from tensor counts), context, tokens (vocab), tensor list. *Well-solved* (candle/gguf crates, `@huggingface/gguf`, python `gguf`). |
| SafeTensors | first 8 bytes = LE header length; inner JSON | header keys (`metadata`, `architectures`, `format`, `quant`), param count from tensor shapes, `__metadata__` (training info). Read via mmap — never whole-file. |
| ONNX | protobuf magic + `onnx` model graph | producer, ir_version, graph io, opset → arch/kind; keep payload small, parse graph I/O only. |
| PyTorch `.bin/.pt/.pth` | **do not unpickle** (RCE). Detect by structure/adjacency instead. | presence/absence, companion `config.json` (arch, hidden_size → params). Never execute pickle. (*This is a hard line, see §17/§18.*) |
| MLX / quantization-aware safetensors | safetensors header (`quantization_config`) | same as safetensors + quant tags. |
| TF/Keras | `saved_model` dir signature / `.h5` | treat as "model dir candidate", shallow. |
| LoRA/adapters | safetensors header with `adapter_model` naming; GGUF `*.lora` | "adapter" role, base hints. |
| Tokenizers/embeddings | names + configs + safetensors shapes | classify kind; `tokenizer.json`/`vocab` attach to a family as tok role. |
| Diffusers dirs | `model_index.json` + submodules | kind=diffusion; variant=subflavour. |

**Filename heuristics** (used only for `inferred` level): a tokeniser over
normalised filenames — strip punctuation/stopwords, recognise quant tags
(`Q4_K_M`, `q8_0`, `iq3_xxs`, `bf16`…), params-tags (`30b`/`30B`/`8b`),
subflavour tags (`instruct`, `base`, `chat`, `v3`, `merged`, `8k`/`32k`),
curator tags to ignore (`bartowski-`, `TheBloke`). Heavy weight on
file-metadata first; filenames only fill gaps, always flagged `inferred`.

**Family key construction (the load-bearing algorithm).** For the three files
in the brief:

```
qwen3-coder-q4.gguf
Qwen3-Coder-30B-Q5_K_M.gguf
Qwen3-Coder-30B-Q8.gguf
```

1. Parse GGUF KV: read `general.name`, `general.basename`, `general.finetune`,
   `general.file_type`, `llama.architecture`, `llama.block_count`,
   `llama.attention.head_count`. → tokens: `{qwen3-coder, 30b-infer-from-config,
   instruct?}` with `known` confidence for everything read.
2. Filename parse gives the same tokens at `inferred` level → agreement boosts.
3. `family_key = slug(normalized(name)) ⊕ arch ⊕ params-class` where the
   params-class normalises 30b-mismatch cases (`general.name` "qwen3-coder"`
   + config says ~30B → family "Qwen3-Coder-30B").
4. Variant = (quant from `file_type` cross-checked against filename tag,
   format=gguf, subflavour from field value).
5. **Merge pass across the whole library**: fuzzy equal-token similarity
   (token Jaccard ≥ threshold w/ params/arch guard rails) proposes merges;
   silent merges only when `known` metadata agrees, otherwise they land in an
   explicit "merge suggestions" review queue. User merge/rename is recorded
   and respected forever after (per-family alias table).

Much of identity is recoverable *locally*. External metadata (HF model card,
gguf-index) is used to (a) fill licence/author/repo/URL — provenance facts —
and (b) *confirm* the family, at `external` level, never overriding local.

**Unknown models are first-class.** Any unidentifiable file becomes an entry
under **Unknown**, fully hashable/browsable, with an "Identify" action:
parsers rerun → sha256 → gguf-index / HF lookup → user may type a name → item
graduates into the catalogue with a `manual` provenance entry. Browsing and
searching must work for unknown items *before* identification succeeds. This
is the "some-random-file.safetensors" UX.

---

## 9. External-source integration

**Principle: provider-agnostic, offline-first, external-marked.** Two plugin
shapes:

- **DiscoverySource** (what/where to look) — MVP: local paths (roots),
  Ollama manifest reader (`~/.ollama/models/manifests` — read-only, resolves
  blob hardlinks so Ollama cache isn't "×3 mystery tera"), LM Studio `models`
  dir structure, HF cache layout (`~/.cache/huggingface/hub/models--*`, incl.
  the `.cache/huggingface` metadata + `snapshots` → instant revision/url). Each
  is just "another root with a smarter default parser".
- **EnrichmentProvider** (facts about a known identity) — MVP:
  - **Hugging Face (read-only)** — by family key / repo guess / filename:
    model card JSON (licence, author, params, tags), file listing, size, and
    `?revision=`. Fetched payloads are cached under `~/.cache/janus/http` with
    ETag; usable offline afterwards.
  - **gguf-index (mozilla-ai)** — **sha256 → (repo_id, revision, filename)**.
    This turns any GGUF into a provenance-rich record with near-zero effort.
    It is the "album-art grab" of the GGUF world.
  - Headless/batch mode maps to providers later: **ModelScope, Civitai
    (diffusion sidecars), Ollama registry (for update checks), HuggingHack-ish
    private mirrors** — all `EnrichmentProvider` impls behind a common
    `providers.enable` flag, pure opt-in.
  - A **hash-lookup service** is also a nice post-MVP pool: communities can
    host sha256↔model caches; Janus just needs one provider shape to talk to.

**Guarantees:** no write to upstream, no account required, every enrichment
row has `provider/fetched_at/etag` and is stored to survive the source dying
forever. Janus must never render "HF is down" as "your library broke."

---

## 10. Offline / removable-storage strategy

The hoarder case (SSD1+SSD2+NAS+drawer) drives this design.

- Each `storage_roots` row is the source of truth for one location,
  identified by `mount_id`, independent of the literal path.
- On startup and on a timer, Janus probes registered roots (stat + read
  first bytes). Missing → `present=0`; all files under it → `state='missing'`
  (they stay in the catalogue with size, hashes, provenance, groupings).
- **Only one root, the true model row.** A model with instances on four drives
  is *four artefacts sharing one family*. If the drawer drive is gone, the
  family shows "present on SSD1, on drawer-drive (offline)".
- **Search/browse/answer questions about offline models**: yes, everything
  except "reveal in file manager". Storage views show offline roots greyed
  with "last seen <date>".
- **Cold/deep storage**: user tags a root `cold` → Janus stops polling it
  every cycle, only rescans on explicit mount events or manual `scan`; avoids
  spinning up a NAS or waking a drive. A `--cold` `scan` flag exists.
- **Mount events**: best-effort via mount notifications on Linux
  (udisks2/DBus), macOS (DiskArbitration), Windows (WM_DEVICECHANGE/volume
  arrival) → immediate re-probe + rescan; graceful fallback to polling.
- **Flaky NAS**: a root that "blips" is kept as `present` with a
  hysteresis (N consecutive failures before `present=0`) so a NFS hiccup
  doesn't mark everything missing and dirty the diff state.

**The mental model is immutable history + reconcilable present.** Janus
remembers what it once saw; a scan never destroys old facts, it adds a
`scan_runs` delta and refreshes `state`.

---

## 11. CLI design

Single binary `janus`. Human tables by default, `--json` everywhere. Grouped
by verb area; derived from the actual workflows rather than a fixed list:

```bash
# roots
janus root add PATH [--name] [--kind nas|removable|internal] [--mode catalogue|managed] [--cold]
janus root ls            janus root rm ID          janus root mount ID    # force re-probe
janus root doctor        # reconcile missing/symlink/dup quirks per root

# scanning / watching
janus scan [ROOT...] [--quick] [--no-hash] [--no-enrich]            # explicit scan
janus watch on|off|status                                           # daemon-set filesystem watch
janus daemon [--api :4321]                                          # watcher + REST + UI server

# the library
janus list [--kind llm|vision|audio|embeddings|adapter|diffusion|unknown]
           [--family] [--root ID] [--offline] [--dups] [--quants-of <fam>]
janus search QUERY [--quant Q4_K_M] [--min-params 8b] [--max-params 30b]
                  [--fits-vram 24] [--source hf|ollama|manual] [--limits ...]
janus show <model|file|id>            # detail: identity, variants, instances, provenance
janus quants <model>                  # variant ladder + what's missing vs HF

# identity & provenance
janus identify FILE [--deep] [--non-interactive]    # parse→hash→lookup→label
janus enrich [--model X | --all] [--provider hf|gguf-index] [--dry-run]
janus merge SRC TARGET [--undo]                    # confirm a family merge suggestion
janus verify <target> [--full]                     # rehash; gguf-index/sha where relevant

# duplicates & storage
janus dedup [--plan | --apply] [--root ...] [--dry-run]   # plan shows reclaimable
janus storage [summary|tree|dups|roots] [--root]           # the 8 TB question
janus cold [mark|unmark] ID

# misc
janus status        janus tag ASSIGN|NEW FAM MODEL   janus export manifest.json
janus import manifest.json   janus doctor   janus completions
```

Notes: `scan` is the only really-owned verb; `list`/`search`/`show` are the
browse surface and share one query engine with the UI. Improperly, I dropped
the brief's `janus duplicates`/`identify unknown.gguf` spellings — they become
`dedup --plan` / `identify` — but kept the intent. `scan` subsumes `import`
(a path you point at *is* a root).

---

## 12. UI/UX design

**Form factor: a local web UI, shipped in the binary.** `janus daemon`
serves a bundled Svelte frontend at `http://localhost:4321` and auto-opens a
browser. Rationale: cross-platform trivially, zero per-OS packaging for the
fun part, one query engine shared with the CLI, PWA-installable feel, and no
Electron bloat. A Tauri shell (native menus, tray, mount events, "open in
file manager") is a later lightweight wrapper — the web app stays the
interface.

**IA (answers the actual questions):**

```
Home            → "What do I actually have?" — big numbers: N models, M TB,
                  X TB reclaimable dups, Y unknown, Z offline; recently added;
                  per-kind breakdown. The "holy shit" landing page.
Library         → browse all models grouped by family; filters: kind, size,
                  params, quant, source, licence, VRAM-fit, date, tag,
                  offline-only, duplicates-only. Grid or compact table.
Model page      → identity card (family / revision / variant ladder — all
                  quants horizontally, "missing vs HF" chips), size, params,
                  context, capabilities tokens, licence+author+repo (with
                  truth-level badges), instances per root (drive, online/off,
                  path), provenance timeline (download events, checksums),
                  related tags, "reveal file", "verify", "merge suggest".
Storage         → treemap by root then family ("what eats my 8 TB"); bars per
                  root; dedup panel showing reclaimable-by-group with
                  per-action delete/hardlink (confirm → journaled).
Unknown         → inbox of unidentifiable files; inline "identify" flow
                  (parse → sha256 → gguf-index/HF lookup → you type a name);
                  accept/decline suggestions.
Duplicates      → blob-exact groups, rename-same-content, near-dup (later),
                  with reclaimable math.
Search          → single box, `Cmd/Ctrl-K`-style; smart chips for
                  `quant:q4_k_m`, `params:<=30b`, `offline`, `fits:24gb`.
```

**Truth-level UI**: every derived field has a tiny badge (known/detected/
inferred/external). Tooltip on hover. This is the anti-bloat discipline —
each field must be (a) true to its level, (b) useful, else drop it. **We do
not render** `likeCount`, uptime, "agent scores", or pipeline graph bloat.

**Beauty criteria for the target user:** dense-but-readable tables, sensible
grouping by family with the variant ladder as the hero component, satisfying
empty-into-organised transition ("before/after" framing on first scan), zero
signup, instant local search, dark theme-first.

---

## 13. Technical architecture

**Language: Rust.** Single static binary; fast multi-TB BLAKE3 hashing; safe
parsing of *untrusted binary metadata* from the internet; mature ecosystem for
everything required; optional Tauri later. (Go is a defensible fallback if the
team prefers — the architecture doesn't change; only this bullet does.)

**Data: SQLite (WAL)** — one file `janus.db`, FTS5, batched writer. Massively
simpler than anything else and *correct for personal scale* (hundreds of
models, tens of thousands of files). No Postgres, no Redis, no S3.

**Libraries:** clap · rusqlite ‑bundled/FTS5 · tokio · notify · walkdir/blake3/
xxhash-rust · memmap2 (header-only format reads) · serde/serde_json · axum
(API + static) · reqwest (enrichment; timeouts+ETag cache) · rust-embed
(frontend). Frontend: Svelte 5, zero-build constrained, embedded.

**Process model (the only "server"):**
- `janus <cmd>` — one-shot CLI; opens DB, runs, exits.
- `janus daemon` — watcher (filesystem events, debounced+jittered), the
  enrichment scheduler, root mount probes, an embedded REST API
  (`/api/v1/{models,files,storage,dups,search,jobs,ingest}`), and static UI.
  State daemon ↔ CLI via the shared SQLite file (single-writer discipline:
  only one process writes jobs/enrich; CLI scans either coordinate via an
  advisory lock or delegate to daemon).

**Concurrency:** workers do *I/O* (walk, hash, parse headers, fetch) in a small
pool (default 4×cores for hashing, 2 for network). They push events into one
writer task that owns SQLite writes (bounded mpsc, batched transactions,
checkpoint every N). Persisted `jobs` rows make any scan resumable after a
crash. Watch uses full-scan-cooldown + per-root last-seen to avoid rehash hot
paths (the stat-cache from §7).

**Failure posture:** writer never blocks readers; WAL keeps daemon UI + CLI
alive concurrently; every pipeline stage degrades (a file that fails to parse
is catalogued `unsupported` with the error — never crashes the run).

**API/plugin shape:** `DiscoverySource` and `EnrichmentProvider` traits (§9).
Nothing else is "pluggable" in MVP. No microservices. No containers for the
app itself (a Docker hop for NAS-only deployments is a README recipe, not a
design goal).

---

## 14. MVP scope

**In — the smallest useful Janus:**
1. `root add/ls`, catalogue-mode scan of arbitrary folders.
2. Format detection (magic) + metadata extraction: **GGUF, safetensors,
   ONNX (graph I/O), companion-`config.json`**; everything else catalogued
   structurally (`detected`).
3. BLAKE3 blob hashing with stat-cache; cross-file + cross-root dedup
   detection; hardlink/symlink aware.
4. Family/revision/variant grouping with the merge-suggestion queue.
5. Offline/removable root tracking (present/missing/cold + hysteresis).
6. FTS5 search + filters (kind, quant, params, size, source, offline, dups).
7. Storage summary + treemap; duplicate reclaim view (report-only; apply =
   user-confirmed delete/hardlink with journal).
8. Unknown inbox + `identify` flow (local parse → optional sha256 →
   optional gguf-index/HF lookup → manual naming).
9. Provenance entries for everything observed (incl. Ollama/LM Studio/HF-cache
   roots as read-only discovery).
10. `janus daemon` + localhost web UI + CLI parity. `export/import` manifest
    for backup/portability.
11. Enrichment **optional & off by default**; provider = HF card + gguf-index;
    cached offline; latest-fetch-fails-never-blocks.

**Explicitly not in MVP** (each has a "how it stays cleanly outside" note):
downloading, inference/launch, HF upload, model conversion/quantization,
managed-mode import flows (design exists, ship later), near-dup tensor-content
matching, Shell/Tauri wrapper, mobile, multi-user, cloud sync.

---

## 15. Post-MVP roadmap

1. **Managed-mode + import-from-downloaders** (materialize `hf` pulls,
   `ollama pull` sidecar tracking, "download into managed").
2. **Near-duplicate detection** via tensor-content signatures; "re-packed
   same weights" grouping.
3. **Missing-quant intelligence** — enrich a family from HF's own file list;
   "you have Q4_K_M, HF offers Q5_K_M/Q8_0" chips (→ one-click acquisition
   *thread* later, not a queue).
4. **More providers**: ModelScope, Civitai sidecars, private mirrors, custom
   sha256-lookup pool; community-curated "family key → canonical name" maps
   (this is the deepest data-moat and the natural open contribution surface).
5. **Update detection** — `aside from modelshelf's update check`: compare
   local revision vs HF `main` without downloading a byte.
6. **REST API + read-only streaming** so other tools (llama-swap, agents)
   can query the catalogue; integration detours into Ollama/llama.cpp/LM
   Studio as *consumers of the catalogue*, never the reverse.
7. **Tauri shell** for native polish + mount events + tray.
8. **Collections/tags/smart views**, saved searches, VRAM-fit guessing
   (params × bytes/param by quant), run/launch shortcuts pointing at
   existing runtimes (llml's trick — *later*).
9. **Cold-storage budget & backup verification** (rehash-on-mount, "verified
   against checksum" status retained through offline periods).

Catalogue stays the spine; every future feature attaches to identity +
provenance, never bolt-on a downloader UI.

---

## 16. Major risks and the hard problems

| Risk | Severity | Mitigation |
|---|---|---|
| **Pickle safety** (torch bug/malware via `.bin/.pt/.pth`) | High | Never unpickle, ever. Structural config-driven inference only; sandboxed subprocess shell-out later if a real need appears. Document loudly. |
| **Filename over-trust → wrong families merged** | High | `known`-first identity; merges only auto when content agrees; review queue; per-family aliases; truth-level badges in UI. Wrong guesses arc recoverable. |
| **Sharded / dir-form models** (safetensors 1-2-3 shards + `index.json`, diffusers trees, MM projector files) | High | Model-DIR detector (config.json + weights dir = one model); `artefacts.role` split; companion/`mmproj`/encoder GGUF exclusion (llamaUI's trick); the "one family from many files" path is first-class. |
| **Hashing cost on 4–10 TB** | Med | Stat-cache, three gates, background hashing that respects I/O load, per-root hashing runs resumed by `jobs`, users choose `--quick` (size+dims only) vs full. |
| **Flaky NAS / mounts → false offline** | Med | Hysteresis, mount events, `cold` roots, never destroys history ({7,10}). |
| **External metadata laundering** | Med | `external` level, separate tables, timestamps, never merges into `known`. Discipline is a coding standard. |
| **Windows specifics** (case-insensitive FS, OneDrive, ADS, reparse points) | Low-Med | Canonical-case keys, mount_id via volume serial, no hard assumptions about case; test on two OSes at least. |
| **DB growth / FTS drift** | Low | Batched writer, FTS delta rebuilding, WAL checkpoint, pruning scan_runs older than N. |
| **Quant-from-ftype ambiguity** (mixed k-quants, MoE weightdistribution) | Low | Store raw `file_type` + filename tag + our normalised quant; show both when they differ. |
| **The "another downloader" trap** (scope creep) | Med | §17 is a hard boundary; product roadmap review gates every quarter. |

---

## 17. Explicit things Janus should NOT build

A hard list, with reasoning (read it again every roadmap cycle):

- **Inference / generation / chat.** Not a runtime. Full stop. (Later: launch
  *existing* runtimes at a catalogue artefact.)
- **OpenAI-compatible API server / serving / router.** llama-swap/llama.cpp/
  vLLM exist. If we ever integrate, we *read* their assets.
- **Download engines in the core flow.** Downloading is a hook bolted onto a
  variant on a family the user already owns; it is not how you discover Janus.
- **Hub upload / publishing.** Sending people's weights somewhere is a
  completely different trust surface.
- **Model conversion/quantization/GPTQ.** AutoGGUF/DASLab et al. own it.
- **Training, fine-tuning, experiment tracking.** Different users entirely.
- **Multi-user accounts, RBAC, cloud sync, S3, Kubernetes, microservices.**
  Personal single-user local-first. (LAN multi-machine is a *post-MVP* catalog
  replica, not an MVP server farm.)
- **Another managed model cache it downloads into by default.** Catalogue by
  default; managed mode is opt-in.
- **Anything that requires the internet to function.** Enrichment is a cache;
  identity works offline.

---

## 18. Verification of the key design questions (from the brief)

**Q. How do the three Qwen files get recognised as one model?**
A. Content-first: parse GGUF KV (`general.name/basename`, `general.file_type`,
arch, block config) → `known`. Filename tokens (`inferred`) agree → family
`Qwen3-Coder-30B`. Variants from `file_type` ⇄ filename cross-check. If two
files genuinely disagree in content, they're *two* family rows offered for
merge, never forced together.

**Q. Unknown models?** First-class "Unknown" bucket; catalogued, hashed,
browseable, searchable *before* identification; then parse→sha256→lookup→name.

**Q. File ownership?** Both: catalogue-in-place (default, read-only) and
managed-mode opt-in import. Never force a reorg to use Janus.

**Q. Same 20 GB on 4 drives?** One variant, four instances. `dedup` considers
them byte-identical (reclaim candidates); Library/Storage shows the family once
with four locations and their presence states. No forced canonicalisation.

**Q. Offline drive?** Root `present=0`; instances `missing`; family browsable,
provenance intact, "last seen". History immutable; reconnect reconciles.

**Q. Variants?** quant / format / subflavour / revision / role (base, instruct,
finetune, merge, LoRA, tokenizer, mmproj, sidecar) — each a row on the variant
ladder with truth levels.

**Q. "What's consuming my 8 TB?"** Storage page: root→family treemap; dedup
panel with reclaimable-per-group; per-family size = sum of distributed per-shard
sizes across instances without double counting same-bytes.

---

## 19. Final recommendation

**Build it.** The catalogue-of-the-present is a genuine, currently-unserved
need, and the workload (scanning, hashing, parsing known formats, SQLite,
a local web UI) is *small enough for one serious engineer to ship an MVP*. The
#1 trap is scope creep toward downloaders/runtimes — the boundary in §17 plus
the catalogue-first everything in this doc is how you avoid becoming "another
Ollama."

**MVP day-one differentiator to optimise the entire design around:** the first
`janus scan` on a messy multi-terabyte collection ends with a Library that
groups by family, a Duplicates tab showing `400 GB reclaimable across 3 drives`,
an Unknown inbox that actually moves files to identified, and greyed "this one's
on the drive in the drawer" states. A datahoarder seeing that screen says
*holy shit, I needed this* — that's the product. Nothing in the downloader-verse
delivers it.

*Where to start coding:* crate layout
`janus-core` (scan/hash/parse/identity/db) → `janus` CLI → `janus daemon` +
web UI. First milestones: (1) scan+hash+format-detect into SQLite,
(2) family/variant grouping, (3) storage+dedup view, (4) offline roots,
(5) `identify` + HF/gguf-index enrichment. Everything else is roadmap.