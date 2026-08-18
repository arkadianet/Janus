# Janus

Local-first library for the AI model files you already own. A second face
can watch providers and fetch what you are actually missing.

**Status:** Phase A–D work in tree (catalogue, loopback UI, radar,
fetch). Not tagged `0.2.0` / `0.3.0` / `1.0.0` until the owner runs the
handwritten [golden hoard](docs/golden-hoard.md) checkboxes on `fedora`.
See [PRODUCT.md](PRODUCT.md) and [ROADMAP.md](ROADMAP.md).

- **Catalogue** (Plex): scan messy folders and drives, including ones that
  are unplugged. Group by family / revision / variant. Say what is known
  vs guessed.
- **Radar + fetch** (Sonarr): wanted / missing / cutoff against a quality
  profile. By default, never download bytes that are already
  **verified-owned** (`have_bytes`); `--force` may redownload that blob.

Janus is not a runtime, chat UI, Hub mirror, or torrent/`*arr` indexer
stack.

## Catalogue (works offline)

```bash
janus root add ~/models
janus scan
janus list
```

Same questions in the browser:

```bash
janus daemon
# open http://127.0.0.1:4321
```

Network is not required for those. Radar and gated fetch are opt-in
and may use a token: copy [example.env](example.env) to `.env` (never
commit it). `HF_TOKEN` is read from the environment only after you
opt in.

```bash
janus profile ls
janus monitor add FAMILY --profile daily-llm
janus radar --once          # lists remote files; does not download
janus wanted
janus root add --kind fetch ~/models/inbound
janus fetch WANTED_ID
```

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
