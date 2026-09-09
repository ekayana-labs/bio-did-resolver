//! Command line definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use did_bio_core::account::vm_flags;

#[derive(Parser, Debug)]
#[command(name = "bio-did-resolver", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Resolve a DID to its DID document; never writes
    Resolve {
        /// The did:bio DID to resolve
        did: String,
        /// RPC endpoint; defaults to the public endpoint of the DID's cluster
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
    /// Create the registry account for a DID; permissionless, any payer
    Init(WriteOpts),
    /// Add a verification method
    AddKey {
        /// Fragment identifier without `#`, e.g. `rotation-1`
        fragment: String,
        /// Key algorithm
        #[arg(long = "type", value_enum)]
        key_type: KeyTypeArg,
        /// Public key as base58
        #[arg(long, value_name = "BASE58", conflicts_with = "key_file")]
        key: Option<String>,
        /// Public key as a raw byte file; use this for ML-DSA-87 keys, which
        /// are uploaded in chunks through a key buffer over several
        /// transactions and resume where they left off if interrupted
        #[arg(long, value_name = "PATH")]
        key_file: Option<PathBuf>,
        /// Comma separated: authentication, assertion, key-agreement,
        /// capability-invocation, capability-delegation, protected
        #[arg(long, value_name = "LIST")]
        flags: String,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Remove a verification method
    RemoveKey {
        fragment: String,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Replace a verification method's relationship flags
    SetFlags {
        fragment: String,
        /// Comma separated; see `add-key --flags`
        #[arg(long, value_name = "LIST")]
        flags: String,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Add a service endpoint
    AddService {
        fragment: String,
        /// Service type, e.g. `BioMetadata`
        service_type: String,
        /// Service endpoint URI, e.g. `ipfs://<cid>`
        endpoint: String,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Remove a service endpoint
    RemoveService {
        fragment: String,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Replace the controller sets
    SetControllers {
        /// A did:bio controller, by Solana public key; repeatable
        #[arg(long = "controller", value_name = "PUBKEY")]
        native: Vec<String>,
        /// A controller from another DID method; repeatable
        #[arg(long = "external", value_name = "DID")]
        other: Vec<String>,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Permanently deactivate a DID; requires --yes
    Deactivate(WriteOpts),
    /// Discard a pending large key upload and reclaim its rent
    CloseKeyBuffer(WriteOpts),
}

/// Options shared by every command that sends a transaction.
#[derive(Args, Debug, Clone)]
pub struct WriteOpts {
    /// The DID to act on; defaults to the keypair's own DID on --network
    pub did: Option<String>,
    /// Keypair that pays for the transaction and signs as update authority
    #[arg(long, value_name = "PATH")]
    pub keypair: Option<PathBuf>,
    /// Cluster of the keypair's own DID when no DID is given
    #[arg(long, value_enum, default_value = "devnet")]
    pub network: NetworkArg,
    /// RPC endpoint; defaults to the public endpoint of the DID's cluster
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// Simulate the transaction and print the result instead of sending it
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm an irreversible action or a mainnet transaction
    #[arg(long)]
    pub yes: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkArg {
    Mainnet,
    Devnet,
    Testnet,
    Localnet,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTypeArg {
    Ed25519,
    X25519,
    Secp256k1,
    #[value(name = "ml-dsa-87")]
    MlDsa87,
}

/// Parse a comma separated relationship list into the program's flag bits.
pub fn parse_flags(list: &str) -> Result<u16, String> {
    let mut flags = 0u16;
    for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        flags |= match item.to_ascii_lowercase().replace('-', "").as_str() {
            "authentication" => vm_flags::AUTHENTICATION,
            "assertion" | "assertionmethod" => vm_flags::ASSERTION,
            "keyagreement" => vm_flags::KEY_AGREEMENT,
            "capabilityinvocation" => vm_flags::CAPABILITY_INVOCATION,
            "capabilitydelegation" => vm_flags::CAPABILITY_DELEGATION,
            "protected" => vm_flags::PROTECTED,
            other => return Err(format!("unknown relationship `{other}`")),
        };
    }
    Ok(flags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn flags_parse_in_any_spelling() {
        assert_eq!(
            parse_flags("authentication,capability-invocation").unwrap(),
            vm_flags::AUTHENTICATION | vm_flags::CAPABILITY_INVOCATION
        );
        assert_eq!(
            parse_flags("assertionMethod, keyAgreement ,protected").unwrap(),
            vm_flags::ASSERTION | vm_flags::KEY_AGREEMENT | vm_flags::PROTECTED
        );
        assert_eq!(parse_flags("").unwrap(), 0);
        assert!(parse_flags("authentication,admin").is_err());
    }
}
