# Janus

Local-first library for the AI model files you already own — with a second
face that can watch providers and fetch what you are actually missing.

- **Catalogue** (Plex): scan messy folders and drives, including ones that
  are unplugged. Group by family / revision / variant. Tell the truth about
  what is known vs guessed.
- **Radar + fetch** (Sonarr): wanted / missing / cutoff against a quality
  profile. By default, never download bytes the catalogue already has;
  `--force` may redownload an already-owned blob.

Janus is not a runtime, chat UI, Hub mirror, or torrent/`*arr` indexer stack.

See [DESIGN.md](DESIGN.md) for the architecture. Copy [example.env](example.env)
to `.env` if you need a Hugging Face token for gated fetches; Janus never
requires the network to catalogue local files.
