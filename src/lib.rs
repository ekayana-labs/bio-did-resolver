//! Resolver and registry client for the `did:bio` DID method.
//!
//! Resolution itself is implemented by `did-bio-core`. This crate adds
//! the RPC transport, the instruction encoding for the registry program
//! ([`ix`]), and the command line definition ([`cli`]) behind the
//! `bio-did-resolver` binary.

#![forbid(unsafe_code)]

pub mod cli;
pub mod ix;
