# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
