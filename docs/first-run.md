# First run

Goal: empty → "I can see my hoard" without reading DESIGN.md.

## Empty

- Home: one sentence ("Add a folder you already keep models in") and
  `Add root`.
- CLI: `janus` with no args prints the three commands and `janus doctor`.
- No account, no token prompt, no "connect Hugging Face."

## Add root

- Catalogue is the default. Explain: Janus will not move these files.
- If volume UUID is missing: offer `.janus-root` opt-in or refuse (do not
  bind by path silently).
- Warn if the path is inside `~/.ollama` or an HF cache: mark
  `kind=discovery`, read-only.

## First scan

- Show progress: files seen, bytes, hash queue ("12 / 400 hashed").
- `--quick` is offered for huge first runs, with the line: grouping yes,
  duplicates/ownership no until a full hash.
- Failures per file (`unsupported`, `partial`) do not abort the run.

## After scan

- Home numbers with inferred split.
- If Unknown > 0: one link to the inbox.
- If reclaimable > 0: show the number, no delete button.
- If a root is already offline: it still appears, grey.

## What we never show on first run

Fetch setup, quality profiles, trending, "download a starter model,"
sign-in.
