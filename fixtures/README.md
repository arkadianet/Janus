# Fixtures

No multi-GB weights in git. Cases are declarative. When Phase A starts,
drop **header-only** or tiny public files next to the case (or fetch into
`fixtures/cache/`, gitignored).

Each `cases/*.toml` maps to [docs/family-key-cases.md](../docs/family-key-cases.md).

```text
janus-core test: load case → run identity on listed files → assert expect
```

Do not delete cases because they are awkward. Awkward is the product.
