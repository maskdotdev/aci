# Contributing

ACI is a Rust workspace. Keep CLI code thin, put reusable behavior in the
library crates, and keep source files below the limits in `AGENTS.md`.

## Setup

Install Rust 1.88 or newer, then validate the workspace:

```sh
./scripts/check-loc.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./scripts/validate-packaging.sh
```

`cargo audit` is recommended before release and in security-sensitive changes.

## Development Notes

- Use deterministic IDs from repository, path, language, symbol kind, qualified
  name, and source span.
- Preserve incremental indexing behavior: hash files, skip unchanged files, and
  replace graph partitions per changed file.
- Add focused tests for language adapters, graph storage, query behavior, and
  export formats when changing those areas.
- Keep language adapters in the `languages/<language>/` layout documented in
  `AGENTS.md` and `docs/adapter-authoring.md`.

## Pull Requests

Before opening a pull request, run the setup validation commands above and
include any relevant CLI smoke-test output. For user-visible CLI changes, update
`README.md` and the relevant document under `docs/`.
