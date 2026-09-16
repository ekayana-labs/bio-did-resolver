# bio-did-resolver

[![CI](https://github.com/ekayana-labs/bio-did-resolver/actions/workflows/main.yml/badge.svg)](https://github.com/ekayana-labs/bio-did-resolver/actions/workflows/main.yml)
[![crates.io](https://img.shields.io/crates/v/bio-did-resolver.svg)](https://crates.io/crates/bio-did-resolver)
[![docs.rs](https://img.shields.io/docsrs/bio-did-resolver)](https://docs.rs/bio-did-resolver)
[![MSRV](https://img.shields.io/crates/msrv/bio-did-resolver)](Cargo.toml)
[![license](https://img.shields.io/crates/l/bio-did-resolver)](LICENSE)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/ekayana-labs/bio-did-resolver/badge)](https://scorecard.dev/viewer/?uri=github.com/ekayana-labs/bio-did-resolver)

Resolver and registry client for the
[`did:bio`](https://github.com/ekayana-labs/did-bio-spec) DID method on
Solana.

| Information | Value |
| --- | --- |
| Registry program | `H1gnV4GjNT3UV7AgGNUCkSaciuVVtM7hKb8JhPV3Xxy6` (devnet) |
| Resolution library | [`did-bio-core`](https://github.com/ekayana-labs/did-bio-core) |
| Program source | [`bio-did-registry`](https://github.com/ekayana-labs/bio-did-registry) |

## Install

```console
cargo install bio-did-resolver
```

Or from source: `cargo install --path .`

## Resolve

```console
bio-did-resolver resolve did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc
```

Prints the full DID Resolution result (`didDocument`, `didDocumentMetadata`
with `versionId` and `updated`, `didResolutionMetadata`). The DID's network
segment selects the cluster; `--url` overrides the RPC endpoint.

A key subject with no registry account resolves to its *generative*
document, `versionId "0"`. An owned subject (see below) has no generative
document: without an account it resolves to `notFound`. An RPC failure is
an error, never a fallback: a node that withholds the account must not be
able to hide a key rotation or a deactivation.

## Update the registry

Every write command signs with a keypair (`--keypair`, default
`~/.config/solana/id.json`) that pays for the transaction and, for updates,
must hold a `capabilityInvocation` method on the DID. The target DID is an
optional last argument; it defaults to the keypair's own DID on `--network`
(default `devnet`).

```console
bio-did-resolver init
bio-did-resolver add-service metadata BioMetadata ipfs://<cid>
bio-did-resolver add-key rotation-1 --type ed25519 --key <BASE58> --flags authentication,capability-invocation
bio-did-resolver add-key pq --type ml-dsa-87 --key-file pq.pub --flags assertion
bio-did-resolver set-flags default --flags authentication,assertion,capability-invocation,protected
bio-did-resolver set-controllers --controller <PUBKEY> --external did:web:lab.example.org
bio-did-resolver remove-service metadata
bio-did-resolver remove-key rotation-1
bio-did-resolver deactivate --yes
```

`init` is permissionless: pass another DID to sponsor its account without
gaining any control over it. The subject has to be a key; the program
refuses an address off the Ed25519 curve, since nothing could ever sign for
it.

```console
bio-did-resolver init did:bio:devnet:<SUBJECT> --keypair sponsor.json
```

## Owned DIDs

A DID does not have to be a key. `init-owned` derives the subject from the
keypair and a nonce (`["bio-did-owned", authority, nonce]`, an off-curve
program address) and creates the account with the keypair as the DID's
protected `#default` method, so one signature names a dataset, paper or
claim the keypair owns and pays for. The same keypair and nonce always name
the same DID; the command prints it.

```console
bio-did-resolver init-owned 42
bio-did-resolver add-service metadata BioMetadata ipfs://<cid> did:bio:devnet:<OWNED>
bio-did-resolver resolve did:bio:devnet:<OWNED>
```

From then on the DID behaves like any other, pass it as the last argument
of the update commands, since it is not the keypair's own DID.

An ML-DSA-87 key is 2592 bytes and a transaction holds 1232, so
`add-key --type ml-dsa-87` uploads the key through a key buffer: one
transaction opens it, three write the chunks, and one appends the method and
refunds the buffer's rent. Run the same command again to resume an
interrupted upload, or discard it:

```console
bio-did-resolver close-key-buffer
```

### Safety

- `--dry-run` simulates the transaction and prints the program logs and
  compute units instead of sending it.
- Sending to mainnet requires `--yes`.
- `deactivate` is permanent and always requires `--yes`.
- Requests the program would refuse fail before they leave the machine,
  with a reason: key material is length checked against the key type,
  fragments follow the program's charset and `default` stays reserved for
  the founding key, `capability-invocation` and `protected` are only
  accepted on `ed25519` keys, and an `--external` controller must be a
  `did:<method>:<id>` of another method.
- When the program does refuse a transaction, its error is printed by name
  and meaning (`InvalidFragment (6002), the fragment is empty, too long,
  reserved, or contains invalid characters`) rather than as a bare code.

## Development

```console
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

`tests/encoding.rs` pins the wire format independently of the program:
discriminators recomputed from instruction names, borsh argument layout,
account metas, the owned subject derivation. `tests/parity.rs` compiles the
program as a host library and checks that this crate, `did-bio-core`, and
the program agree on every constant, derivation and error code.

## License

[MIT](LICENSE)
