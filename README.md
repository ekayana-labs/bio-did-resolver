# bio-did-resolver

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

A DID with no registry account resolves to its *generative* document,
`versionId "0"`. An RPC failure is an error, never a fallback: a node that
withholds the account must not be able to hide a key rotation or a
deactivation.

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
gaining any control over it.

```console
bio-did-resolver init did:bio:devnet:<SUBJECT> --keypair sponsor.json
```

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
- Key material is length checked against the key type before it leaves the
  machine; the program enforces the same rule on chain.

## Development

```console
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

`tests/encoding.rs` pins the wire format independently of the program:
discriminators recomputed from instruction names, borsh argument layout,
account metas. `tests/parity.rs` compiles the program as a host library and
checks that this crate, `did-bio-core`, and the program agree on every
constant.

## License

[MIT](LICENSE)
