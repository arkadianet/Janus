# Security

Janus reads untrusted model files from disk and, if you opt in, talks to
model providers. Catalogue must work with the network unplugged.

## What never happens

- Janus never unpickles `.bin` / `.pt` / `.pth`. No sandbox exception.
- Parsers cap GGUF KV and safetensors header size. No mmap of device files.
- Walks do not follow symlinks out of a registered root.
- Discovery roots (Ollama, LM Studio, HF cache) are never written.
- Fetch writes only under the single fetch root after `dest_rel_path`
  validation (no `..`, no absolute/UNC/drive paths, no symlink parents).
- `janus daemon --api` binds loopback only unless authentication, TLS (or
  equivalent), and request-origin controls are all enabled.

## What leaves the machine

| Action | Network |
|---|---|
| `scan`, `list`, `show`, `--quick` | Nothing |
| `identify --deep` / gguf-index | Opt-in. SHA-256 of a file *you chose* |
| `enrich` / `radar` | Opt-in. Repo ids or hashes you asked to look up |
| `fetch` | Opt-in. HTTPS to the provider; optional `HF_TOKEN` for gated repos |

Scan does not upload filenames or a hoard fingerprint. A hash lookup of
local weights is still a fingerprint of that file — the UI must say so
before it runs.

Do not put tokens in the repo. Copy [example.env](example.env) to `.env`
locally. `.env` is gitignored.

## Trusting the catalogue

`--quick` and `hash_state=none` files are unverified. They must not
satisfy `have_bytes`, `skipped_have_bytes`, or fetch-suppression.
`size` + `mtime` + `ino` is not proof of unchanged bytes.

## Reporting

Open a private report to the author if you find a parse crash, path
escape, or a way to make fetch write outside the fetch root. Do not file
that as a public "feature" issue.
