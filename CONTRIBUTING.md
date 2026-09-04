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

## Rules of the road

- **The program is the source of truth.** Instruction discriminators,
  argument layout, and account order in `src/ix.rs` mirror the program;
  change them only together with a program change, and update
  `tests/encoding.rs` and `tests/parity.rs` in the same PR.
- **Resolution logic lives in `did-bio-core`.** This crate only adds
  transport and commands; a resolution behavior change belongs upstream.
- **Never fall back on transport failure.** An RPC error must surface as an
  error, not as the generative document.
- **Writes are explicit.** New commands that send transactions print what
  they are about to do, honor `--dry-run`, and require `--yes` for anything
  irreversible or bound for mainnet.

## Commit messages

Short, capitalized, imperative subject with no trailing period:
`Add key rotation commands`, `Fix explorer link for localnet`. Use a
`ci:`, `docs:`, `deps:`, or `chore:` prefix only for mechanical changes.
