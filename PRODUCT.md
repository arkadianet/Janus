# Janus — v1 product sheet

This page is the law for **done**. [DESIGN.md](DESIGN.md) is the law for
**how**. If they disagree, stop and fix one of them.

**Status:** specified, not implemented. No binary yet.

---

## Promise

Figure out what you already have. Tell the truth about known vs guessed.
Do not move a file. Do not need the internet. Do not download a file the
catalogue already knows (unless `--force`).

Janus is a catalogue first, an acquirer second, a runtime never.

**v1 honesty:** LLM / GGUF first. Well-behaved safetensors and ONNX too.
Diffusion / ComfyUI / Civitai piles are files (often Unknown), not the
identity target.

---

## Who it is for

Someone with models scattered across folders, app caches, a NAS, and a
drive in a drawer. They can use a CLI. They do not want another chat app.

User zero is the author. Ship when *they* would throw away the spreadsheet.

---

## v1 is done when (Phase A + B)

Run against the [golden hoard](docs/golden-hoard.md). Every line must hold.

1. `janus root add` on real folders, including one you can unplug, plus
   read-only discovery of Ollama / LM Studio / HF cache if present.
2. `janus scan` finishes without moving files. Remove Janus: folders
   unchanged (no `.janus-root` unless the user opted in).
3. The handwritten family / Unknown / dup / offline answers on the golden
   hoard still match after a second scan.
4. Home / `janus list` counts split inferred vs known. Inferred is never
   shown as known.
5. `janus storage` and `janus dedup --plan` show a reclaimable number
   computed from unique `(mount_id, dev, ino)`, not `size × (N−1)`.
6. Unplugging a root greys it out. The family stays. Radar/search treat
   **verified** blobs on that root as owned. Reveal-in-file-manager is the
   only thing that needs the drive present.
7. Discovery roots (Ollama, LM Studio, HF cache) are never written.
8. `janus identify` on an unknown file can name it (`manual`). Export
   includes aliases and declined merges.
9. Scan works with the network unplugged.
10. Someone else can follow the three commands in the README without
    reading DESIGN.md.

**Not required for v1:** radar, fetch, daemon-as-the-only-way-to-live,
managed reorg, delete/hardlink apply, Tauri, VRAM-fit, launch shortcuts.

Radar (Phase C) and fetch (Phase D) make a *full* Janus. They are the next
product, not the first.

---

## What v1 will not do

See DESIGN.md §16 never-list. Short version: no chat, no serve, no Hub
upload, no convert, no pickle, no torrent/`*arr` indexers, no default
managed download cache, no mutating app caches.

---

## How we know we are lying

- Home says "N models" and most of them are inferred, with no split.
- Two different 8B Llamas became one family with no merge prompt.
- Q4 and Q8 of the same basename did *not* become one family.
- Reclaimable counts hardlinked Ollama blobs as freeable disk.
- A `--quick` scan suppressed a fetch because "we already have it."
- Unplug a NAS and every file row flipped to `missing` (write storm).

If any of those happen, v1 is not done. UI polish does not fix them.
