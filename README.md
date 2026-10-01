# bio-did-resolver

[![CI](https://github.com/ekayana-labs/bio-did-resolver/actions/workflows/main.yml/badge.svg)](https://github.com/ekayana-labs/bio-did-resolver/actions/workflows/main.yml)
[![crates.io](https://img.shields.io/crates/v/bio-did-resolver.svg)](https://crates.io/crates/bio-did-resolver)
[![docs.rs](https://img.shields.io/docsrs/bio-did-resolver)](https://docs.rs/bio-did-resolver)
[![MSRV](https://img.shields.io/crates/msrv/bio-did-resolver)](Cargo.toml)
[![license](https://img.shields.io/crates/l/bio-did-resolver)](LICENSE)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/ekayana-labs/bio-did-resolver/badge)](https://scorecard.dev/viewer/?uri=github.com/ekayana-labs/bio-did-resolver)

Resolver and registry client for the
[`did:bio`](https://github.com/ekayana-labs/bio-did-spec) DID method on
Solana. It resolves a DID to its DID document and sends the registry
program's instructions from the command line.

| Information | Value |
| --- | --- |
| Registry program | `H1gnV4GjNT3UV7AgGNUCkSaciuVVtM7hKb8JhPV3Xxy6` (devnet) |
| Resolution library | [`did-bio-core`](https://github.com/ekayana-labs/did-bio-core) |
| Program source | [`bio-did-registry`](https://github.com/ekayana-labs/bio-did-registry) |

## Install

```console
cargo install bio-did-resolver
```

To build from a checkout, run `cargo install --path .`.

## Resolve

```console
bio-did-resolver resolve did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc
```

The command prints the full DID Resolution result. This DID has no
registry account, so it resolves to its generative document.

```json
{
  "didResolutionMetadata": { "contentType": "application/did+ld+json" },
  "didDocument": {
    "id": "did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc",
    "verificationMethod": [
      {
        "id": "did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc#default",
        "type": "Multikey",
        "publicKeyMultibase": "z6MkfuN2vWAoHermh6vY6TgAJfwhBWZCApZb9XeQ9HwLqHfz"
      }
    ],
    "authentication": ["did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc#default"]
  },
  "didDocumentMetadata": { "deactivated": false, "versionId": "0" }
}
```

The output above is shortened. The DID's network segment selects the
cluster, and `--url` overrides the RPC endpoint. A registered DID also
reports `updated` and a `versionId` that grows with every write.

A key subject with no registry account resolves to its generative
document, `versionId "0"`. An owned subject has no generative document, so
without an account it resolves to `notFound`. An RPC failure is always an
error and never falls back to the generative document. A node that
withheld the account could otherwise hide a key rotation or a
deactivation.

Reads use `finalized` commitment, as the spec recommends. Pass
`--commitment confirmed` to see a write about 13 seconds sooner, at the
risk of a one slot rollback.

## Dereference, sign and verify

`dereference` resolves the DID of a DID URL and prints what its fragment
names. That is a verification method or a service, since fragments are
unique across both. Without a fragment it prints the whole document.

```console
bio-did-resolver dereference did:bio:devnet:<ID>#default
bio-did-resolver dereference did:bio:devnet:<ID>#metadata
```

`verify` checks that the method a DID URL names signed a message and holds
a verification relationship in the resolved document, `authentication` by
default. It handles Ed25519 and ML-DSA-87 methods. `sign` produces a
signature with a Solana keypair to test against.

```console
SIG=$(bio-did-resolver sign --message challenge)
bio-did-resolver verify did:bio:devnet:<ID>#default --message challenge --signature "$SIG"
bio-did-resolver verify did:bio:devnet:<ID>#pq --message-file doc.bin --signature-file doc.sig --relationship assertion-method
```

`did` prints a keypair's DID and registry account without touching the
network. With `--owned <NONCE>` it prints the owned DID that `init-owned`
would create.

```console
bio-did-resolver did
bio-did-resolver did --owned 42
```

## Update the registry

| Command | What it sends |
| --- | --- |
| `init` | Creates the registry account for a key subject |
| `init-owned <NONCE>` | Creates an owned DID, described below |
| `add-key` / `remove-key` | Adds or removes a verification method |
| `set-flags` | Replaces a method's relationship flags |
| `add-service` / `update-service` / `remove-service` | Adds, replaces or removes a service endpoint |
| `set-controllers` | Replaces the native and external controller sets |
| `deactivate --yes` | Deactivates the DID for good |
| `close-key-buffer` | Discards a pending large key upload |

Every write command signs with a keypair, `--keypair`, which defaults to
`~/.config/solana/id.json`. The keypair pays for the transaction. For an
update it must also hold a `capabilityInvocation` method on the DID. The
target DID is an optional last argument. Without it the command acts on
the keypair's own DID on `--network`, which defaults to `devnet`.

```console
bio-did-resolver init
bio-did-resolver add-service metadata BioMetadata ipfs://<cid>
bio-did-resolver update-service metadata BioMetadata ipfs://<new-cid>
bio-did-resolver add-key rotation-1 --type ed25519 --key <BASE58> --flags authentication,capability-invocation
bio-did-resolver add-key pq --type ml-dsa-87 --key-file pq.pub --flags assertion
bio-did-resolver set-flags default --flags authentication,assertion,capability-invocation,protected
bio-did-resolver set-controllers --controller <PUBKEY> --external did:web:lab.example.org
bio-did-resolver remove-service metadata
bio-did-resolver remove-key rotation-1
bio-did-resolver deactivate --yes
```

`init` is permissionless. Pass another DID to sponsor its account without
gaining any control over it. The subject has to be a key, because the
program refuses an address off the Ed25519 curve that nothing could ever
sign for.

```console
bio-did-resolver init did:bio:devnet:<SUBJECT> --keypair sponsor.json
```

A native controller's authority can update the DID too. Pass the
controller's DID with `--via`, and the keypair signs as one of the
controller's `capabilityInvocation` methods. Control reaches one level,
and a `protected` method still changes only under its own key.

```console
bio-did-resolver add-service notes Note https://lab.example.org/notes did:bio:devnet:<DATASET> --via did:bio:devnet:<LAB>
```

## Owned DIDs

A DID does not have to be a key. `init-owned` derives the subject from the
keypair and a nonce as the program address
`["bio-did-owned", authority, nonce]`, which lies off the curve. It creates
the account with the keypair as the DID's protected `#default` method, so
one signature names a dataset, paper or claim that the keypair owns and
pays for. The same keypair and nonce always name the same DID, and the
command prints it.

```console
bio-did-resolver init-owned 42
bio-did-resolver add-service metadata BioMetadata ipfs://<cid> did:bio:devnet:<OWNED>
bio-did-resolver resolve did:bio:devnet:<OWNED>
```

From then on the DID behaves like any other. Pass it as the last argument
of the update commands, since it is not the keypair's own DID.

## Post-quantum keys

An ML-DSA-87 key is 2592 bytes and a transaction holds 1232, so
`add-key --type ml-dsa-87` uploads the key through a key buffer. One
transaction opens the buffer, three write the chunks, and one appends the
method and refunds the buffer's rent. Run the same command again to resume
an interrupted upload, or discard it.

```console
bio-did-resolver close-key-buffer
```

## Safety

- `--dry-run` simulates the transaction and prints the program logs and
  compute units instead of sending it.
- `--json` prints one JSON object per command, with the signatures,
  explorer links, logs and compute units, and the error when it fails.
- Sending to mainnet requires `--yes`.
- `deactivate` is permanent and always requires `--yes`.
- Requests the program would refuse fail before they leave the machine,
  with a reason. Key material is length checked against the key type. An
  `ed25519` key must lie on the curve, and a `secp256k1` key must be
  compressed. Fragments follow the program's charset, and `default` stays
  reserved for the founding key. Only `ed25519` keys may take
  `capability-invocation` or `protected`. A `protected` method keeps
  `capability-invocation`, and only its own key can add it. An `ml-dsa-87`
  key cannot take `key-agreement`. An `--external` controller must be a
  `did:<method>:<id>` of another method, and `--via` must name a native
  controller that the keypair is an authority of.
- When the program does refuse a transaction, its error is printed by name
  and meaning, as in `InvalidFragment (6002), the fragment is empty, too
  long, reserved, or contains invalid characters`.

## Development

```console
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

`tests/encoding.rs` pins the wire format independently of the program. It
recomputes discriminators from instruction names and checks the borsh
argument layout, the account metas and the owned subject derivation.
`tests/parity.rs` compiles the program as a host library and checks that
this crate, `did-bio-core` and the program agree on every constant,
derivation, error code and instruction. It also checks that the client
refuses exactly the flags and keys the program refuses.

`scripts/cluster-tests.sh` runs every instruction and every guard against
a live cluster with throwaway keys. It needs `solana`, `solana-keygen` and
`jq`. Run it against `solana-test-validator` with the program loaded, or
with `NET=devnet` and a funded keypair in `FUNDER`.

## License

[MIT](LICENSE)
