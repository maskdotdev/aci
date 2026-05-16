# Release Checklist

- Confirm `cargo fmt --all -- --check`.
- Confirm `cargo clippy --workspace --all-targets -- -D warnings`.
- Confirm `cargo test --workspace`.
- Confirm `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
- Confirm `./scripts/validate-packaging.sh`.
- Confirm `cargo audit`.
- Confirm `./scripts/check-loc.sh`.
- Confirm `sh -n site/install.sh`.
- Run `aci index <path>` on a representative repository.
- Export JSONL and verify it imports cleanly.
- Regenerate benchmark baselines when publishing benchmark claims.
- If publishing crates, publish in dependency order starting with `aci-core`;
  downstream workspace crates cannot complete full publish dry-runs until their
  sibling dependencies exist in the registry.
- Review `PLAN.md` for unchecked release blockers.
