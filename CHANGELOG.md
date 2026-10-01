# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-10-01

### Added

- `update-service` replaces a service's type and endpoint in place.
- `--via <DID>` signs an update as an authority of a native controller of
  the DID.

### Changed

- Built against `did-bio-core` and `bio-did-registry` 0.2. Program error
  6018, `InvalidKey`, is printed by name.
- `WriteOpts` gained a field and `Command` a variant.
- The client refuses what registry 0.2 refuses. That covers a `protected`
  method without `capability-invocation` or under another key,
  `key-agreement` on an ML-DSA-87 key, an Ed25519 key off the curve and a
  secp256k1 key that is not compressed.

## [0.2.0] - 2026-10-01

### Added

- `dereference <did-url>` prints the verification method or service a DID
  URL names, or the whole document without a fragment.
- `verify <did-url>` checks a signature against a method in the resolved
  document and the relationship it must hold. It handles Ed25519 and
  ML-DSA-87 methods through `did-bio-core`'s `verify` feature.
- `sign` signs a message with a Solana keypair and prints the signature.
- `did` prints a keypair's DID and registry account, or with `--owned` the
  owned DID for a nonce, without touching the network.
- `--commitment` chooses `finalized` or `confirmed` for the read commands.
- `--json` on every write command prints one JSON object with the
  signatures, explorer links, logs and compute units.

### Changed

- Moved to `solana-client` 4.3 with `solana-sdk` 4.1.
- The command enum and the option structs gained variants and fields.
- `set-flags` checks the new flags against the method's key type before
  sending, as `add-key` already did.
- `init-owned` reads the keypair file once.

### Fixed

- Localnet explorer links carry the RPC endpoint, so the explorer opens
  the right cluster.
- The cluster tests expect the client's own messages for the requests it
  now refuses before sending, and cover owned DIDs.
- CI checks that `init-owned` is reachable.

## [0.1.1] - 2026-09-17

### Added

- `init-owned <NONCE>` creates an owned DID, whose subject is derived from
  the keypair and a nonce and whose `#default` method is the keypair.
- Requests the program would refuse fail on the client with a reason, and
  program errors are printed by name and meaning.

### Changed

- Built against `did-bio-core` and `bio-did-registry` 0.1.2.

## [0.1.0] - 2026-09-09

First release, with `resolve`, the registry update commands and chunked
upload of ML-DSA-87 keys through a key buffer.

[Unreleased]: https://github.com/ekayana-labs/bio-did-resolver/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ekayana-labs/bio-did-resolver/releases/tag/v0.3.0
[0.2.0]: https://github.com/ekayana-labs/bio-did-resolver/releases/tag/v0.2.0
[0.1.1]: https://github.com/ekayana-labs/bio-did-resolver/releases/tag/v0.1.1
[0.1.0]: https://github.com/ekayana-labs/bio-did-resolver/releases/tag/v0.1.0
