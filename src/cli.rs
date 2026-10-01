//! Command line definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use did_bio_core::account::{
    vm_flags, KeyType, DEFAULT_FRAGMENT, MAX_CONTROLLER_LEN, MAX_FRAGMENT_LEN,
};
use did_bio_core::VerificationRelationship;

#[derive(Parser, Debug)]
#[command(name = "bio-did-resolver", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Resolve a DID to its DID document without writing anything
    Resolve {
        /// The did:bio DID to resolve
        did: String,
        #[command(flatten)]
        read: ReadOpts,
    },
    /// Print the verification method or service a DID URL names
    Dereference {
        /// A DID URL such as `did:bio:devnet:<ID>#default`. Without a
        /// fragment the whole document is printed
        did_url: String,
        #[command(flatten)]
        read: ReadOpts,
    },
    /// Check a signature against a verification method in the resolved
    /// document
    Verify {
        /// The signing method as a DID URL, such as `did:bio:devnet:<ID>#default`
        did_url: String,
        /// The signed message as UTF-8 text
        #[arg(
            long,
            value_name = "TEXT",
            conflicts_with = "message_file",
            required_unless_present = "message_file"
        )]
        message: Option<String>,
        /// The signed message as a raw byte file
        #[arg(long, value_name = "PATH")]
        message_file: Option<PathBuf>,
        /// The signature as base58
        #[arg(
            long,
            value_name = "BASE58",
            conflicts_with = "signature_file",
            required_unless_present = "signature_file"
        )]
        signature: Option<String>,
        /// The signature as a raw byte file, handy for 4627 byte ML-DSA-87
        /// signatures
        #[arg(long, value_name = "PATH")]
        signature_file: Option<PathBuf>,
        /// The relationship the method must hold in the document
        #[arg(long, value_enum, default_value = "authentication")]
        relationship: RelationshipArg,
        #[command(flatten)]
        read: ReadOpts,
    },
    /// Sign a message with a keypair and print the signature as base58, the
    /// counterpart of `verify`
    Sign {
        /// The message as UTF-8 text
        #[arg(
            long,
            value_name = "TEXT",
            conflicts_with = "message_file",
            required_unless_present = "message_file"
        )]
        message: Option<String>,
        /// The message as a raw byte file
        #[arg(long, value_name = "PATH")]
        message_file: Option<PathBuf>,
        /// Keypair to sign with, by default `~/.config/solana/id.json`
        #[arg(long, value_name = "PATH")]
        keypair: Option<PathBuf>,
    },
    /// Print a keypair's DID and registry account without touching the network
    Did {
        /// Print the owned DID this keypair creates with this nonce instead
        #[arg(long, value_name = "NONCE")]
        owned: Option<u64>,
        /// Keypair to derive from, by default `~/.config/solana/id.json`
        #[arg(long, value_name = "PATH")]
        keypair: Option<PathBuf>,
        /// Cluster the DID names
        #[arg(long, value_enum, default_value = "devnet")]
        network: NetworkArg,
    },
    /// Create the registry account for a DID, paid for by anyone
    Init(WriteOpts),
    /// Create an owned DID whose subject is derived from the keypair and a
    /// nonce, controlled by the keypair from the first version
    InitOwned {
        /// Nonce that names the DID together with the keypair. The same nonce
        /// always names the same DID
        nonce: u64,
        #[command(flatten)]
        write: OwnedOpts,
    },
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
        /// Public key as a raw byte file. Use it for ML-DSA-87 keys, which go
        /// up in chunks through a key buffer over several transactions and
        /// resume where they stopped if interrupted
        #[arg(long, value_name = "PATH")]
        key_file: Option<PathBuf>,
        /// Comma separated list of authentication, assertion, key-agreement,
        /// capability-invocation, capability-delegation and protected
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
        /// Comma separated, as for `add-key --flags`
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
    /// Replace a service's type and endpoint, keeping its place in the
    /// document
    UpdateService {
        fragment: String,
        /// New service type, e.g. `BioMetadata`
        service_type: String,
        /// New service endpoint URI, e.g. `ipfs://<cid>`
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
        /// A did:bio controller given by its Solana public key, repeatable
        #[arg(long = "controller", value_name = "PUBKEY")]
        native: Vec<String>,
        /// A controller from another DID method, repeatable
        #[arg(long = "external", value_name = "DID")]
        other: Vec<String>,
        #[command(flatten)]
        write: WriteOpts,
    },
    /// Permanently deactivate a DID, which requires --yes
    Deactivate(WriteOpts),
    /// Discard a pending large key upload and reclaim its rent
    CloseKeyBuffer(WriteOpts),
}

/// Options shared by the commands that read the registry.
#[derive(Args, Debug, Clone)]
pub struct ReadOpts {
    /// RPC endpoint, by default the public endpoint of the DID's cluster
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// Commitment to read at. `confirmed` sees writes about 13 seconds
    /// sooner and may still roll back one slot
    #[arg(long, value_enum, default_value = "finalized")]
    pub commitment: CommitmentArg,
}

/// Options shared by every command that sends a transaction.
#[derive(Args, Debug, Clone)]
pub struct WriteOpts {
    /// The DID to act on, by default the keypair's own DID on --network
    pub did: Option<String>,
    /// Keypair that pays for the transaction and signs as update authority
    #[arg(long, value_name = "PATH")]
    pub keypair: Option<PathBuf>,
    /// Cluster of the keypair's own DID when no DID is given
    #[arg(long, value_enum, default_value = "devnet")]
    pub network: NetworkArg,
    /// RPC endpoint, by default the public endpoint of the DID's cluster
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// Simulate the transaction and print the result instead of sending it
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm an irreversible action or a mainnet transaction
    #[arg(long)]
    pub yes: bool,
    /// Print one JSON object with the signatures and logs instead of text
    #[arg(long)]
    pub json: bool,
    /// Sign as an authority of this native controller of the DID, given as
    /// its did:bio. The program then checks the controller's own methods
    #[arg(long, value_name = "DID")]
    pub via: Option<String>,
}

/// Options of `init-owned`, which derives the DID instead of taking one.
#[derive(Args, Debug, Clone)]
pub struct OwnedOpts {
    /// Keypair that pays for the transaction and becomes the DID's authority
    #[arg(long, value_name = "PATH")]
    pub keypair: Option<PathBuf>,
    /// Cluster the DID lives on
    #[arg(long, value_enum, default_value = "devnet")]
    pub network: NetworkArg,
    /// RPC endpoint, by default the public endpoint of the cluster
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// Simulate the transaction and print the result instead of sending it
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm a mainnet transaction
    #[arg(long)]
    pub yes: bool,
    /// Print one JSON object with the signatures and logs instead of text
    #[arg(long)]
    pub json: bool,
}

impl OwnedOpts {
    /// The same options as a write to a given DID.
    pub fn for_did(&self, did: String) -> WriteOpts {
        WriteOpts {
            did: Some(did),
            keypair: self.keypair.clone(),
            network: self.network,
            url: self.url.clone(),
            dry_run: self.dry_run,
            yes: self.yes,
            json: self.json,
            via: None,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkArg {
    Mainnet,
    Devnet,
    Testnet,
    Localnet,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentArg {
    Finalized,
    Confirmed,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipArg {
    Authentication,
    AssertionMethod,
    KeyAgreement,
    CapabilityInvocation,
    CapabilityDelegation,
}

impl From<RelationshipArg> for VerificationRelationship {
    fn from(arg: RelationshipArg) -> Self {
        match arg {
            RelationshipArg::Authentication => VerificationRelationship::Authentication,
            RelationshipArg::AssertionMethod => VerificationRelationship::AssertionMethod,
            RelationshipArg::KeyAgreement => VerificationRelationship::KeyAgreement,
            RelationshipArg::CapabilityInvocation => VerificationRelationship::CapabilityInvocation,
            RelationshipArg::CapabilityDelegation => VerificationRelationship::CapabilityDelegation,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTypeArg {
    Ed25519,
    X25519,
    Secp256k1,
    #[value(name = "ml-dsa-87")]
    MlDsa87,
}

impl From<KeyTypeArg> for KeyType {
    fn from(arg: KeyTypeArg) -> Self {
        match arg {
            KeyTypeArg::Ed25519 => KeyType::Ed25519,
            KeyTypeArg::X25519 => KeyType::X25519,
            KeyTypeArg::Secp256k1 => KeyType::Secp256k1,
            KeyTypeArg::MlDsa87 => KeyType::MlDsa87,
        }
    }
}

// The checks below mirror the program's own, so a request the registry
// would refuse fails here with a reason instead of on chain with a code.

/// The key type as the command line names it.
fn key_type_name(key_type: KeyType) -> &'static str {
    match key_type {
        KeyType::Ed25519 => "ed25519",
        KeyType::X25519 => "x25519",
        KeyType::Secp256k1 => "secp256k1",
        KeyType::MlDsa87 => "ml-dsa-87",
        _ => key_type.on_chain_name(),
    }
}

/// Check a fragment the way the program does. It is 1 to 32 characters of
/// `[A-Za-z0-9_-]` and never `default`, which names the founding key.
pub fn check_fragment(fragment: &str) -> Result<(), String> {
    if fragment.is_empty() || fragment.len() > MAX_FRAGMENT_LEN {
        return Err(format!(
            "fragment must be 1 to {MAX_FRAGMENT_LEN} characters, got {}",
            fragment.len()
        ));
    }
    if !fragment
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!(
            "fragment `{fragment}` may only contain letters, digits, `_` and `-`"
        ));
    }
    if fragment == DEFAULT_FRAGMENT {
        return Err(format!(
            "`{DEFAULT_FRAGMENT}` is reserved for the founding key, pick another fragment"
        ));
    }
    Ok(())
}

/// Check flags against a key type the way the program does. Only Ed25519
/// keys can sign a transaction, so only they may hold
/// `capability-invocation` or be `protected`, and a protected method keeps
/// `capability-invocation` so that its own key can always change it. An
/// X25519 key only agrees on keys, and an ML-DSA-87 key only signs.
pub fn check_flags(key_type: KeyType, flags: u16) -> Result<(), String> {
    if flags & !vm_flags::VALID_MASK != 0 {
        return Err("unknown flag bits".into());
    }
    if key_type != KeyType::Ed25519 {
        if flags & vm_flags::CAPABILITY_INVOCATION != 0 {
            return Err(format!(
                "only ed25519 keys can hold capability-invocation, since {} keys cannot sign a transaction",
                key_type_name(key_type)
            ));
        }
        if flags & vm_flags::PROTECTED != 0 {
            return Err(format!(
                "only ed25519 keys can be protected, since {} keys cannot sign for themselves",
                key_type_name(key_type)
            ));
        }
    }
    if flags & vm_flags::PROTECTED != 0 && flags & vm_flags::CAPABILITY_INVOCATION == 0 {
        return Err(
            "a protected method must keep capability-invocation, so that its own key can change it"
                .into(),
        );
    }
    if key_type == KeyType::X25519
        && flags & vm_flags::RELATIONSHIP_MASK & !vm_flags::KEY_AGREEMENT != 0
    {
        return Err("an X25519 key can only carry key-agreement".into());
    }
    if key_type == KeyType::MlDsa87 && flags & vm_flags::KEY_AGREEMENT != 0 {
        return Err("an ML-DSA-87 key only signs, so it cannot carry key-agreement".into());
    }
    Ok(())
}

/// Check key bytes of the right length the way the program does. A
/// protected method only goes in under its own key. An Ed25519 key is a
/// point on the curve unless it is the signer itself, which is how a program
/// address holds authority through CPI. A secp256k1 key is a compressed
/// point, starting with 0x02 or 0x03.
pub fn check_key(
    key_type: KeyType,
    flags: u16,
    key: &[u8],
    signer: &[u8; 32],
) -> Result<(), String> {
    if flags & vm_flags::PROTECTED != 0 && key != signer {
        return Err(
            "a protected method can only be added under its own key, so sign with that key".into(),
        );
    }
    let on_curve = || <&[u8; 32]>::try_from(key).is_ok_and(did_bio_core::is_on_curve);
    match key_type {
        KeyType::Ed25519 if key != signer && !on_curve() => {
            Err("the ed25519 key is not a point on the curve, so nothing could sign with it".into())
        }
        KeyType::Secp256k1 if !matches!(key.first(), Some(0x02 | 0x03)) => {
            Err("a secp256k1 key must be a compressed point, starting with 0x02 or 0x03".into())
        }
        _ => Ok(()),
    }
}

/// Check an external controller the way the program does. It is a
/// `did:<method>:<id>` with a lowercase alphanumeric method and a non-empty
/// id, in printable ASCII of at most 128 bytes. did:bio controllers are passed by
/// key with `--controller` instead.
pub fn check_external_controller(did: &str) -> Result<(), String> {
    if did.is_empty() || did.len() > MAX_CONTROLLER_LEN {
        return Err(format!(
            "controller must be 1 to {MAX_CONTROLLER_LEN} bytes, got {}",
            did.len()
        ));
    }
    if !did.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(format!(
            "controller `{did}` must be printable ASCII without whitespace"
        ));
    }
    let shape = did
        .strip_prefix("did:")
        .and_then(|rest| rest.split_once(':'))
        .filter(|(method, id)| {
            !method.is_empty()
                && method
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                && !id.is_empty()
        });
    match shape {
        None => Err(format!(
            "controller `{did}` is not a DID of the form did:<method>:<id>"
        )),
        Some(("bio", _)) => Err(format!(
            "`{did}` is a did:bio, pass it by key with --controller instead"
        )),
        Some(_) => Ok(()),
    }
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
    fn verify_needs_a_message_and_a_signature() {
        let did = "did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc#default";
        assert!(
            Cli::try_parse_from(["bio-did-resolver", "verify", did, "--message", "m"]).is_err()
        );
        assert!(
            Cli::try_parse_from(["bio-did-resolver", "verify", did, "--signature", "s"]).is_err()
        );
        let parsed = Cli::try_parse_from([
            "bio-did-resolver",
            "verify",
            did,
            "--message",
            "m",
            "--signature",
            "s",
            "--relationship",
            "assertion-method",
            "--commitment",
            "confirmed",
        ])
        .unwrap();
        let Command::Verify {
            relationship, read, ..
        } = parsed.command
        else {
            panic!("parsed as another command");
        };
        assert_eq!(relationship, RelationshipArg::AssertionMethod);
        assert_eq!(read.commitment, CommitmentArg::Confirmed);
    }

    #[test]
    fn fragments_follow_the_program_rules() {
        assert!(check_fragment("rotation-1").is_ok());
        assert!(check_fragment(&"f".repeat(32)).is_ok());
        assert!(check_fragment("").is_err());
        assert!(check_fragment(&"f".repeat(33)).is_err());
        assert!(check_fragment("no spaces").is_err());
        assert!(check_fragment("default").unwrap_err().contains("reserved"));
    }

    #[test]
    fn flags_follow_the_key_type() {
        let sign = vm_flags::AUTHENTICATION | vm_flags::CAPABILITY_INVOCATION;
        assert!(check_flags(KeyType::Ed25519, sign | vm_flags::PROTECTED).is_ok());
        assert!(check_flags(KeyType::MlDsa87, vm_flags::ASSERTION).is_ok());
        assert!(check_flags(KeyType::MlDsa87, vm_flags::CAPABILITY_INVOCATION).is_err());
        assert!(check_flags(KeyType::MlDsa87, vm_flags::ASSERTION | vm_flags::PROTECTED).is_err());
        assert!(check_flags(KeyType::X25519, vm_flags::KEY_AGREEMENT).is_ok());
        assert!(check_flags(KeyType::X25519, vm_flags::AUTHENTICATION).is_err());
        assert!(check_flags(KeyType::Ed25519, 1 << 12).is_err());
        assert!(check_flags(
            KeyType::Ed25519,
            vm_flags::AUTHENTICATION | vm_flags::PROTECTED
        )
        .unwrap_err()
        .contains("must keep capability-invocation"));
        assert!(check_flags(KeyType::MlDsa87, vm_flags::KEY_AGREEMENT)
            .unwrap_err()
            .contains("only signs"));
    }

    #[test]
    fn keys_follow_the_program_rules() {
        let point: [u8; 32] = bs58::decode("2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc")
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap();
        // A program derived address is never a curve point.
        let address =
            crate::ix::did_account(&solana_sdk::pubkey::Pubkey::new_from_array(point)).to_bytes();
        let signer = [9u8; 32];
        let auth = vm_flags::AUTHENTICATION;
        let protected = vm_flags::CAPABILITY_INVOCATION | vm_flags::PROTECTED;
        assert!(check_key(KeyType::Ed25519, auth, &point, &signer).is_ok());
        assert!(check_key(KeyType::Ed25519, auth, &address, &signer)
            .unwrap_err()
            .contains("not a point on the curve"));
        assert!(check_key(KeyType::Ed25519, protected, &address, &address).is_ok());
        assert!(check_key(KeyType::Ed25519, protected, &point, &signer)
            .unwrap_err()
            .contains("its own key"));
        let mut secp = [7u8; 33];
        secp[0] = 0x03;
        assert!(check_key(KeyType::Secp256k1, auth, &secp, &signer).is_ok());
        secp[0] = 0x04;
        assert!(check_key(KeyType::Secp256k1, auth, &secp, &signer)
            .unwrap_err()
            .contains("compressed"));
        assert!(check_key(KeyType::MlDsa87, vm_flags::ASSERTION, &[1; 2592], &signer).is_ok());
    }

    #[test]
    fn external_controllers_are_dids_of_other_methods() {
        assert!(check_external_controller("did:web:lab.example.org").is_ok());
        assert!(check_external_controller("did:key:z6Mk").is_ok());
        for bad in [
            "",
            "did:",
            "did:web",
            "did:web:",
            "did:Web:x",
            "web:lab",
            "did:web:a b",
        ] {
            assert!(check_external_controller(bad).is_err(), "{bad}");
        }
        assert!(check_external_controller(
            "did:bio:devnet:2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc"
        )
        .unwrap_err()
        .contains("--controller"));
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
