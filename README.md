# Janus

Local-first library for the AI model files you already own. A second face
can watch providers and fetch what you are actually missing.

**Status:** architecture and product specs only. There is no `janus`
binary yet. See [PRODUCT.md](PRODUCT.md) for what v1 means and
[ROADMAP.md](ROADMAP.md) for phases.

- **Catalogue** (Plex): scan messy folders and drives, including ones that
  are unplugged. Group by family / revision / variant. Say what is known
  vs guessed.
- **Radar + fetch** (Sonarr): wanted / missing / cutoff against a quality
  profile. By default, never download bytes that are already
  **verified-owned** (`have_bytes`); `--force` may redownload that blob.

Janus is not a runtime, chat UI, Hub mirror, or torrent/`*arr` indexer
stack.

## When a binary exists

```bash
janus root add ~/models
janus scan
janus list
```

Network is not required for those three. Gated fetch later may use a
token: copy [example.env](example.env) to `.env` (never commit it).

## Docs

| Doc | What it is |
|---|---|
| [PRODUCT.md](PRODUCT.md) | v1 done / not done |
| [DESIGN.md](DESIGN.md) | Architecture (how) |
| [ROADMAP.md](ROADMAP.md) | Phases A–D |
| [docs/README.md](docs/README.md) | Specs index (CLI, UI, API, schema, fixtures) |
| [SECURITY.md](SECURITY.md) | Privacy and hard lines |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to work on this |

## License

[MIT](LICENSE) · author arkadianet
