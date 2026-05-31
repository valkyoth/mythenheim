# Pull Request

## Summary

Describe what changed and why.

## Type

- [ ] Bug fix
- [ ] Feature
- [ ] Documentation
- [ ] Refactor
- [ ] Dependency update
- [ ] Security hardening
- [ ] Release or deployment change

## Checklist

- [ ] I kept the change scoped to Mythenheim's existing architecture.
- [ ] I updated docs, examples, or roadmap entries when behavior changed.
- [ ] I added or updated tests for behavior changes.
- [ ] I ran `cargo fmt --all --check`.
- [ ] I ran `scripts/validate-release-metadata.sh` when version/toolchain/release docs changed.
- [ ] I ran `perl scripts/check-doc-links.pl` when docs changed.
- [ ] I ran `cargo clippy --all-targets -- -D warnings`.
- [ ] I ran `cargo test`.
- [ ] I considered Linux/macOS/Windows binary portability for runtime changes.
- [ ] I ran `python3 scripts/build_release_binary.py linux --repo . --ref HEAD --allow-untagged` for release packaging changes.
- [ ] I ran `scripts/smoke_local.sh` for config or CLI changes.
- [ ] I ran `scripts/smoke_fluxheim_wolfi.sh` for Fluxheim/proxy/container changes.
- [ ] I checked dependency/license impact when adding or updating crates.
- [ ] I did not commit secrets, private keys, tokens, local runtime data, or generated artifacts.

## Security Notes

Describe any security-sensitive impact. Mention auth, sessions, permissions,
moderation, SurrealDB, user content, plugins, themes, logging, metrics, tracing,
or dependency changes if they are touched.

## Follow-Up

List any known remaining work or intentionally deferred tasks.
