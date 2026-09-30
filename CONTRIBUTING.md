# Contributing

## Development

```console
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

The parity tests compile the registry program crate as a host library.
Both it and `did-bio-core` come from crates.io. To work against unpublished
changes in sibling checkouts, put a patch section in `.cargo/config.toml`
and leave that file untracked:

```toml
[patch.crates-io]
did-bio-core = { path = "../did-bio-core" }
bio-did-registry = { path = "../bio-did-registry/program" }
```

`.cargo/audit.toml` is tracked. It lists the advisories cargo-audit ignores
in CI, each with its reason and the condition for dropping it.

## Rules

- The program is the source of truth. Instruction discriminators, argument
  layout and account order in `src/ix.rs` mirror it. Change them only
  together with a program change, and update `tests/encoding.rs` and
  `tests/parity.rs` in the same PR.
- Resolution logic lives in `did-bio-core`. This crate only adds transport
  and commands, so a change in resolution behavior belongs upstream.
- A transport failure never falls back. An RPC error surfaces as an error
  and never as the generative document.
- Writes are explicit. A new command that sends transactions prints what it
  is about to do, honors `--dry-run`, and requires `--yes` for anything
  irreversible or bound for mainnet.

## Commit messages

Write a short, capitalized, imperative subject with no trailing period,
such as `Add key rotation commands` or `Fix explorer link for localnet`.
Use a `ci:`, `docs:`, `deps:` or `chore:` prefix only for mechanical
changes.
