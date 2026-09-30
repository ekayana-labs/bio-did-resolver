# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

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

[Unreleased]: https://github.com/ekayana-labs/bio-did-resolver/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/ekayana-labs/bio-did-resolver/releases/tag/v0.1.1
[0.1.0]: https://github.com/ekayana-labs/bio-did-resolver/releases/tag/v0.1.0
