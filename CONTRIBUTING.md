# Contributing to Bothy

Thanks for helping! Bug reports, docs fixes, tests and code are all welcome. This page explains
how the repository is laid out, how to build and test each part, and how changes get in.

## Layout

| Path | What |
|---|---|
| `crates/bothy` | the sync engine (Rust) |
| `crates/bothy-cli` | the `bothy` command |
| `crates/bothy-oauth` | Google sign-in helpers |
| `crates/bothy-ffi` | the C ABI |
| `crates/bothy-conformance` | generates and checks `spec/vectors` |
| `bindings/flutter` | the Flutter package (flutter_rust_bridge) |
| `bindings/python` | the Python package (pyo3, maturin) |
| `bindings/node` | the Node.js package (napi-rs), early; not published yet |
| `examples/` | the Flutter demo app, the robot agent, two-machine CLI setup |
| `site/` | the documentation site, <https://bothysync.dev> |
| `spec/vectors` | conformance vectors for other implementations |

## Build and test

You need Rust (stable; the crates support 1.88+). Flutter, Python and Node are only needed for
their parts.

```sh
cargo test --workspace                                      # Rust: engine, CLI, OAuth, C ABI
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo test -p bothy --features test-util --test simulation --release   # 500 seeded scenarios
cargo run -p bothy-conformance -- check spec/vectors        # after a format change: generate
crates/bothy-ffi/tests/run_smoke.sh                         # C smoke test
cargo deny check                                            # licences and advisories

cd bindings/python && maturin develop --release && python -m unittest discover -s tests
cd bindings/node && npm ci && npm run build && npm test
cd bindings/flutter && cargo build --release --manifest-path rust/Cargo.toml && flutter test
cd site && npm ci && npm run build && npm run check-links
```

Tests that touch Google Drive use a mock server. Nothing in the test suite needs a Google
account.

## How changes get in

1. **Open an issue first** (or comment on one) for anything beyond a typo. Describe the
   problem rather than the solution: where it is (`file:line`), why it matters, and what you
   would like to see. Small, focused issues are easier to pick up.
2. **Branch** from `main`: `fix/…`, `feat/…`, `docs/…`, `chore/…`, `test/…` or `ci/…`.
3. **Commit** with [Conventional Commits](https://www.conventionalcommits.org/):
   `fix(core): …`, `feat(cli): …`, `docs(site): …`.
4. **Open a pull request** that says `Closes #N`, with a **What changes** and a **Tested**
   section. CI runs on Linux, macOS and Windows, plus cross builds, the simulation, Flutter and
   Python.
5. A maintainer **squash-merges** once CI is green and the review is done.

## Guidelines

- **The on-remote format is a contract.** A change that affects what is written to Drive needs
  a format note and new or updated vectors. A released format version never changes its reading
  rules.
- **Never log secrets:** tokens, client secrets, passwords, data keys and document contents
  must never be printed or logged.
- **No `unsafe`** outside `crates/bothy-ffi`.
- **Public text is English.** Write plainly, and explain the why in comments only where the
  code cannot.
- **Tests:** add a test that fails without your change.

## Security

Please report vulnerabilities privately; see [SECURITY.md](SECURITY.md).

## Licence

By contributing, you agree that your contributions are dual licensed under MIT or Apache-2.0,
like the rest of the project.
