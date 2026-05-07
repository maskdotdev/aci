# Release Checklist

- Confirm `cargo fmt --all -- --check`.
- Confirm `cargo clippy --workspace --all-targets -- -D warnings`.
- Confirm `cargo test --workspace`.
- Confirm `./scripts/check-loc.sh`.
- Run `aci index <path>` on a representative repository.
- Export JSONL and verify it imports cleanly.
- Review `PLAN.md` for unchecked release blockers.
