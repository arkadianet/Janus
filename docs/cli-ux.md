# CLI UX

Human tables by default. `--json` on every command (same payload the UI
will use). No color-only meaning.

Until a binary exists, this is the contract for Phase A–D. Phase A
implements the catalogue block only.

## Output rules

- One family per `list` row unless `--quants-of` or `--dups`.
- Truth: suffix inferred fields with `~` in the TTY, e.g. `Q4_K_M~`.
  JSON uses `"quant": {"value":"Q4_K_M","level":"inferred"}`.
- Offline roots: name in brackets, e.g. `[drawer]`.
- Unverified hash: `hash:none` — never "owned."
- Errors: stable `code` from [errors.md](errors.md); human line on stderr.

## Catalogue (Phase A)

```text
$ janus root ls
ID  NAME     KIND        PRESENT  COLD  PATH
1   models   internal    yes      no    /home/you/models
2   drawer   removable   no       yes   /media/backup

$ janus list
FAMILY              KIND   PARAMS     VARIANTS          SIZE    ROOTS
Qwen3-Coder-30B     llm    30.5B      Q4_K_M Q5_K_M Q8  48.2G   models, [drawer]
unknown/a1b2c3      ?      —          1 file            2.1G    models

$ janus show Qwen3-Coder-30B
Family   Qwen3-Coder-30B    key=qwen3-coder-30b|llama|t30.5|aunk
Kind     llm (known)
Variants
  Q4_K_M  gguf  instruct  bartowski   18.1G  models  present
  Q8_0    gguf  instruct  bartowski   30.1G  drawer  offline  last seen 2026-08-01
Provenance  downloaded_from hf:Qwen/…  (external)
Evidence    arch=llama known  quant=Q4_K_M known

$ janus storage
ROOT     PRESENT  FILES  BYTES     RECLAIMABLE
models   yes      140    3.1T      410G
drawer   no       80     1.8T      0  (offline; not in reclaim apply)

$ janus dedup --plan
BLOB     SIZE    COPIES  INODES  RECLAIMABLE  PATHS
abc…     20.0G   3       2       20.0G        models/a.gguf, models/copy.gguf
                                 (third path same inode as first — not extra)
```

`dedup --apply` is not Phase A.

## Identity

`janus identify FILE` prints parse → hash state → optional lookup
(opt-in) → prompt for a name unless `--non-interactive`.

`janus merge SRC TARGET` records aliases. `janus merge --decline A B`
writes `declined_merges`.

## Radar / fetch (Phase C / D)

```text
$ janus wanted
ID  FAMILY            REV     FILE                     STATUS              NOTE
12  Qwen3-Coder-30B   main    *-Q5_K_M.gguf            open
13  Qwen3-Coder-30B   main    *-Q8_0.gguf              skipped_have_bytes  drawer (offline)
```

`janus fetch 12` only if `sha256` is present and dest validates.

## JSON shape (shared)

```json
{
  "family": {
    "id": 1,
    "family_key": "qwen3-coder-30b|llama|t30.5|aunk",
    "name": {"value": "Qwen3-Coder-30B", "level": "known"},
    "kind": {"value": "llm", "level": "known"}
  },
  "variants": [],
  "files": [],
  "counts": {"families": 128, "families_inferred": 41}
}
```

Field wrappers `{value, level}` are required for anything that is not a
surrogate key or a raw hash.
