//! The `bio-did-resolver` binary resolves did:bio DIDs and drives the
//! registry program.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use did_bio_core::account::{DidAccountState, KeyType, StoredVerificationMethod};
use did_bio_core::verify::verify_for_relationship;
use did_bio_core::{
    resolution_error, resolve_from_account, BioDid, DidDocument, DidResolution, DidUrl,
    KeyBufferState, Network, RawAccount,
};
use serde_json::{json, Value};
use solana_client::client_error::ClientError;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::instruction::{Instruction, InstructionError};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair, Signer};
use solana_sdk::transaction::{Transaction, TransactionError};

use bio_did_resolver::cli::{
    check_external_controller, check_flags, check_fragment, parse_flags, Cli, Command,
    CommitmentArg, NetworkArg, ReadOpts, RelationshipArg, WriteOpts,
};
use bio_did_resolver::ix;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Resolve { did, read } => resolve(&did, &read),
        Command::Dereference { did_url, read } => dereference(&did_url, &read),
        Command::Verify {
            did_url,
            message,
            message_file,
            signature,
            signature_file,
            relationship,
            read,
        } => {
            let message = load_message(message, message_file.as_deref())?;
            let signature = match (signature, signature_file) {
                (Some(text), None) => bs58::decode(&text)
                    .into_vec()
                    .map_err(|e| anyhow!("--signature is not valid base58: {e}"))?,
                (None, Some(path)) => {
                    fs::read(&path).with_context(|| format!("reading {}", path.display()))?
                }
                _ => bail!("pass exactly one of --signature or --signature-file"),
            };
            verify(&did_url, &message, &signature, relationship, &read)
        }
        Command::Sign {
            message,
            message_file,
            keypair,
        } => {
            let message = load_message(message, message_file.as_deref())?;
            let keypair = load_keypair(keypair.as_deref())?;
            println!("{}", keypair.sign_message(&message));
            Ok(())
        }
        Command::Did {
            owned,
            keypair,
            network,
        } => {
            // Everything here is derived locally, so no RPC endpoint is needed.
            let keypair = load_keypair(keypair.as_deref())?;
            let authority = keypair.pubkey();
            let network = network_of(network);
            let did = match owned {
                Some(nonce) => BioDid::owned(network, &authority.to_bytes(), nonce),
                None => BioDid::new(network, authority.to_bytes()),
            };
            println!("did      {did}");
            println!(
                "account  {}",
                ix::did_account(&Pubkey::new_from_array(did.subject))
            );
            println!("key      {authority}");
            if let Some(nonce) = owned {
                println!("nonce    {nonce}");
            }
            Ok(())
        }
        Command::Init(opts) => {
            let w = Write::new(&opts)?;
            if !w.did.is_key_subject() {
                bail!(
                    "{} is an owned subject rather than a key, and its authority creates \
                     it with `init-owned <NONCE>`",
                    w.did
                );
            }
            w.send("initialize", ix::initialize(&w.payer(), &w.subject))
        }
        Command::InitOwned { nonce, write } => {
            // The DID follows from the keypair and the nonce. The keypair
            // signs as the authority and pays.
            let keypair = load_keypair(write.keypair.as_deref())?;
            let did = BioDid::owned(
                network_of(write.network),
                &keypair.pubkey().to_bytes(),
                nonce,
            );
            let w = Write::with_keypair(&write.for_did(did.to_string()), keypair)?;
            w.send_with(
                "initialize_owned",
                &[("nonce", nonce.to_string())],
                ix::initialize_owned(&w.payer(), &w.payer(), nonce),
            )
        }
        Command::AddKey {
            fragment,
            key_type,
            key,
            key_file,
            flags,
            write,
        } => {
            let w = Write::new(&write)?;
            let key_type = KeyType::from(key_type);
            check_fragment(&fragment).map_err(|e| anyhow!(e))?;
            let key = load_key(key.as_deref(), key_file.as_deref(), key_type)?;
            let flags = parse_flags(&flags).map_err(|e| anyhow!(e))?;
            check_flags(key_type, flags).map_err(|e| anyhow!(e))?;
            if key.len() > ix::MAX_INLINE_KEY_LEN {
                return w.finish(w.upload_key(&fragment, key_type, flags, &key));
            }
            let vm = ix::VerificationMethod {
                fragment: &fragment,
                key_type: key_type as u8,
                flags,
                key: &key,
            };
            w.send(
                "add_verification_method",
                ix::add_verification_method(&w.payer(), &w.payer(), &w.subject, &vm),
            )
        }
        Command::RemoveKey { fragment, write } => {
            let w = Write::new(&write)?;
            w.send(
                "remove_verification_method",
                ix::remove_verification_method(&w.payer(), &w.payer(), &w.subject, &fragment),
            )
        }
        Command::SetFlags {
            fragment,
            flags,
            write,
        } => {
            let w = Write::new(&write)?;
            let flags = parse_flags(&flags).map_err(|e| anyhow!(e))?;
            // Check the flags against the method's key type, as add-key does.
            // An unknown method is left for the program to report.
            if let Some(method) = w.stored_method(&fragment)? {
                check_flags(method.method_type, flags).map_err(|e| anyhow!(e))?;
            }
            w.send(
                "set_verification_method_flags",
                ix::set_verification_method_flags(&w.payer(), &w.subject, &fragment, flags),
            )
        }
        Command::AddService {
            fragment,
            service_type,
            endpoint,
            write,
        } => {
            let w = Write::new(&write)?;
            check_fragment(&fragment).map_err(|e| anyhow!(e))?;
            let service = ix::Service {
                fragment: &fragment,
                service_type: &service_type,
                endpoint: &endpoint,
            };
            w.send(
                "add_service",
                ix::add_service(&w.payer(), &w.payer(), &w.subject, &service),
            )
        }
        Command::RemoveService { fragment, write } => {
            let w = Write::new(&write)?;
            w.send(
                "remove_service",
                ix::remove_service(&w.payer(), &w.payer(), &w.subject, &fragment),
            )
        }
        Command::SetControllers {
            native,
            other,
            write,
        } => {
            let w = Write::new(&write)?;
            let native = native
                .iter()
                .map(|s| {
                    s.parse::<Pubkey>()
                        .with_context(|| format!("invalid controller `{s}`"))
                })
                .collect::<Result<Vec<_>>>()?;
            for did in &other {
                check_external_controller(did).map_err(|e| anyhow!(e))?;
            }
            w.send(
                "set_controllers",
                ix::set_controllers(&w.payer(), &w.payer(), &w.subject, &native, &other),
            )
        }
        Command::Deactivate(opts) => {
            let w = Write::new(&opts)?;
            if !w.yes && !w.dry_run {
                bail!("deactivation is permanent, pass --yes to confirm");
            }
            w.send(
                "deactivate",
                ix::deactivate(&w.payer(), &w.payer(), &w.subject),
            )
        }
        Command::CloseKeyBuffer(opts) => {
            let w = Write::new(&opts)?;
            w.send(
                "close_key_buffer",
                ix::close_key_buffer(&w.payer(), &w.payer(), &w.subject),
            )
        }
    }
}

fn commitment_of(arg: CommitmentArg) -> CommitmentConfig {
    match arg {
        CommitmentArg::Finalized => CommitmentConfig::finalized(),
        CommitmentArg::Confirmed => CommitmentConfig::confirmed(),
    }
}

/// Fetch the registry account for `did` and run the resolution algorithm
/// on it.
///
/// Spec Section 6.2 step 6 permits the generative fallback only for an
/// account that is really absent. An RPC failure is reported as an error
/// and never downgraded to the generative document.
fn fetch_resolution(did: &BioDid, read: &ReadOpts) -> Result<DidResolution> {
    let url = read
        .url
        .clone()
        .unwrap_or_else(|| did.network.default_rpc_url().to_string());
    let commitment = commitment_of(read.commitment);
    let client = RpcClient::new_with_commitment(url, commitment);
    let address = ix::did_account(&Pubkey::new_from_array(did.subject));
    let account = client
        .get_account_with_commitment(&address, commitment)
        .context("fetching the registry account")?
        .value
        .map(|account| RawAccount {
            owner: account.owner.to_bytes(),
            data: account.data,
        });
    Ok(resolve_from_account(did, account.as_ref()))
}

/// The document of a successful resolution, or the resolution error.
fn document_of(did: &BioDid, resolution: DidResolution) -> Result<DidDocument> {
    if let Some(code) = &resolution.resolution_metadata.error {
        if code == resolution_error::NOT_FOUND && !did.is_key_subject() {
            bail!(
                "resolution failed: {code}. An owned subject has no generative document \
                 and resolves once its authority runs `init-owned`"
            );
        }
        bail!("resolution failed: {code}");
    }
    resolution
        .document
        .ok_or_else(|| anyhow!("resolution returned no document"))
}

/// Resolve a DID and print the DID Resolution result as JSON.
fn resolve(did: &str, read: &ReadOpts) -> Result<()> {
    let did: BioDid = did.parse().map_err(|e| anyhow!("invalid DID: {e}"))?;
    let resolution = fetch_resolution(&did, read)?;
    println!("{}", serde_json::to_string_pretty(&resolution)?);
    document_of(&did, resolution).map(|_| ())
}

/// Resolve the DID of a DID URL and print what its fragment names.
fn dereference(did_url: &str, read: &ReadOpts) -> Result<()> {
    let url = DidUrl::parse(did_url).map_err(|e| anyhow!("{e}"))?;
    let document = document_of(&url.did, fetch_resolution(&url.did, read)?)?;
    let value = match &url.fragment {
        None => serde_json::to_value(&document)?,
        Some(fragment) => fragment_of(&document, fragment).ok_or_else(|| {
            anyhow!(
                "dereferencing failed: notFound, {did_url} names no verification method or service"
            )
        })?,
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// The verification method or service a fragment names. Fragments are
/// unique across both, so at most one matches (spec Section 5.4).
fn fragment_of(document: &DidDocument, fragment: &str) -> Option<Value> {
    if let Some(method) = document.verification_method_by_fragment(fragment) {
        return serde_json::to_value(method).ok();
    }
    document
        .service_by_fragment(fragment)
        .and_then(|service| serde_json::to_value(service).ok())
}

/// The W3C name of a verification relationship.
fn relationship_name(relationship: RelationshipArg) -> &'static str {
    match relationship {
        RelationshipArg::Authentication => "authentication",
        RelationshipArg::AssertionMethod => "assertionMethod",
        RelationshipArg::KeyAgreement => "keyAgreement",
        RelationshipArg::CapabilityInvocation => "capabilityInvocation",
        RelationshipArg::CapabilityDelegation => "capabilityDelegation",
    }
}

/// Check that the method a DID URL names holds `relationship` in the
/// resolved document and signed `message`.
fn verify(
    did_url: &str,
    message: &[u8],
    signature: &[u8],
    relationship: RelationshipArg,
    read: &ReadOpts,
) -> Result<()> {
    let url = DidUrl::parse(did_url).map_err(|e| anyhow!("{e}"))?;
    let Some(fragment) = &url.fragment else {
        bail!(
            "name the signing method with a fragment, for example {}#default",
            url.did
        );
    };
    let document = document_of(&url.did, fetch_resolution(&url.did, read)?)?;
    verify_signature(&document, fragment, relationship, message, signature)?;
    println!(
        "valid  {did_url} signed the message under {}",
        relationship_name(relationship)
    );
    Ok(())
}

fn verify_signature(
    document: &DidDocument,
    fragment: &str,
    relationship: RelationshipArg,
    message: &[u8],
    signature: &[u8],
) -> Result<()> {
    verify_for_relationship(document, relationship.into(), fragment, message, signature)
        .map_err(|e| anyhow!("signature rejected: {e}"))
}

/// A transaction error with the registry's own errors spelled out, so a
/// refusal reads as a reason rather than a code.
fn describe(err: &TransactionError) -> String {
    if let TransactionError::InstructionError(_, InstructionError::Custom(code)) = err {
        if let Some((name, meaning)) = ix::program_error(*code) {
            return format!("{err}: {name} ({code}), {meaning}");
        }
    }
    err.to_string()
}

/// An RPC client error, described the same way when it carries a
/// transaction error.
fn describe_client_error(err: ClientError) -> anyhow::Error {
    match err.get_transaction_error() {
        Some(tx_err) => anyhow!("{}", describe(&tx_err)),
        None => anyhow::Error::from(err),
    }
}

/// What a write command did, collected for `--json`.
#[derive(Default)]
struct Report {
    action: String,
    details: serde_json::Map<String, Value>,
    steps: Vec<String>,
    logs: Vec<String>,
    compute_units: Vec<u64>,
    signatures: Vec<Value>,
}

/// Everything a write command needs, which is the target DID, the signing
/// keypair and where to send the transaction.
struct Write {
    did: BioDid,
    subject: Pubkey,
    keypair: Keypair,
    url: String,
    dry_run: bool,
    yes: bool,
    json: bool,
    report: RefCell<Report>,
}

impl Write {
    fn new(opts: &WriteOpts) -> Result<Self> {
        Self::with_keypair(opts, load_keypair(opts.keypair.as_deref())?)
    }

    /// [`Write::new`] with a keypair the caller has already loaded.
    fn with_keypair(opts: &WriteOpts, keypair: Keypair) -> Result<Self> {
        let did = match &opts.did {
            Some(did) => did
                .parse::<BioDid>()
                .map_err(|e| anyhow!("invalid DID: {e}"))?,
            None => BioDid::new(network_of(opts.network), keypair.pubkey().to_bytes()),
        };
        let url = opts
            .url
            .clone()
            .unwrap_or_else(|| did.network.default_rpc_url().to_string());
        Ok(Write {
            subject: Pubkey::new_from_array(did.subject),
            did,
            keypair,
            url,
            dry_run: opts.dry_run,
            yes: opts.yes,
            json: opts.json,
            report: RefCell::default(),
        })
    }

    /// The keypair pays and, for updates, signs as the update authority.
    fn payer(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    /// Send one instruction as one transaction (or simulate it).
    fn send(&self, action: &str, instruction: Instruction) -> Result<()> {
        self.send_with(action, &[], instruction)
    }

    /// [`Write::send`] with extra lines in the announcement.
    fn send_with(
        &self,
        action: &str,
        details: &[(&str, String)],
        instruction: Instruction,
    ) -> Result<()> {
        let result = self.guard(action).and_then(|()| {
            self.announce(action);
            for (label, value) in details {
                self.detail(label, value.clone());
            }
            self.execute(&self.client(), instruction)
        });
        self.finish(result)
    }

    /// Mainnet needs an explicit --yes unless only simulating.
    fn guard(&self, action: &str) -> Result<()> {
        if self.did.network == Network::Mainnet && !self.dry_run && !self.yes {
            bail!("refusing to send `{action}` to mainnet without --yes");
        }
        Ok(())
    }

    fn announce(&self, action: &str) {
        if self.json {
            self.report.borrow_mut().action = action.to_string();
            return;
        }
        println!("{action}");
        println!("  did      {}", self.did);
        println!("  account  {}", ix::did_account(&self.subject));
        println!("  signer   {}", self.keypair.pubkey());
        println!("  rpc      {}", self.url);
    }

    fn detail(&self, label: &str, value: String) {
        if self.json {
            self.report
                .borrow_mut()
                .details
                .insert(label.to_string(), Value::String(value));
        } else {
            println!("  {label:<8} {value}");
        }
    }

    /// A progress line of a multi-transaction write.
    fn step(&self, text: String) {
        if self.json {
            self.report.borrow_mut().steps.push(text);
        } else {
            println!("  {text}");
        }
    }

    /// Print the collected report under --json, with the error if the write
    /// failed, and pass the result on.
    fn finish(&self, result: Result<()>) -> Result<()> {
        if self.json {
            let report = self.report.take();
            let mut out = json!({
                "action": report.action,
                "did": self.did.to_string(),
                "account": ix::did_account(&self.subject).to_string(),
                "signer": self.keypair.pubkey().to_string(),
                "rpc": self.url,
                "dryRun": self.dry_run,
                "ok": result.is_ok(),
                "signatures": report.signatures,
                "logs": report.logs,
                "computeUnits": report.compute_units,
            });
            if !report.details.is_empty() {
                out["details"] = Value::Object(report.details);
            }
            if !report.steps.is_empty() {
                out["steps"] = json!(report.steps);
            }
            if let Err(e) = &result {
                out["error"] = Value::String(format!("{e:#}"));
            }
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        result
    }

    fn client(&self) -> RpcClient {
        RpcClient::new_with_commitment(self.url.clone(), CommitmentConfig::confirmed())
    }

    /// Sign and send one instruction, or simulate it under --dry-run.
    fn execute(&self, client: &RpcClient, instruction: Instruction) -> Result<()> {
        let blockhash = client
            .get_latest_blockhash()
            .context("fetching a recent blockhash")?;
        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            blockhash,
        );

        if self.dry_run {
            let result = client
                .simulate_transaction(&tx)
                .context("simulating the transaction")?
                .value;
            let logs = result.logs.unwrap_or_default();
            if self.json {
                let mut report = self.report.borrow_mut();
                report.logs.extend(logs);
                report.compute_units.extend(result.units_consumed);
            } else {
                for line in logs {
                    println!("  {line}");
                }
                if let Some(units) = result.units_consumed {
                    println!("  compute units {units}");
                }
            }
            return match result.err {
                Some(err) => Err(anyhow!(
                    "simulation failed: {}",
                    describe(&TransactionError::from(err))
                )),
                None => Ok(()),
            };
        }

        let signature = client
            .send_and_confirm_transaction(&tx)
            .map_err(describe_client_error)
            .context("sending the transaction")?;
        let explorer = explorer_url(self.did.network, &signature.to_string(), &self.url);
        if self.json {
            self.report.borrow_mut().signatures.push(json!({
                "signature": signature.to_string(),
                "explorer": explorer,
            }));
        } else {
            println!("  signature {signature}");
            println!("  {explorer}");
        }
        Ok(())
    }

    /// Add a method whose key does not fit in one transaction. It opens a
    /// key buffer or picks up the pending one, writes the key in chunks, then
    /// appends the method and closes the buffer. Every step is a transaction
    /// of its own, so an interrupted upload resumes where it stopped.
    fn upload_key(&self, fragment: &str, key_type: KeyType, flags: u16, key: &[u8]) -> Result<()> {
        self.guard("add_verification_method_from_buffer")?;
        self.announce("add_verification_method (through a key buffer)");
        let client = self.client();
        let payer = self.payer();
        let buffer = ix::key_buffer(&self.subject, &payer);
        self.detail("buffer", buffer.to_string());
        let chunks = key.len().div_ceil(ix::KEY_CHUNK_LEN);

        let written = match self.pending_upload(&client, &buffer)? {
            Some(pending) => {
                let same = pending.fragment == fragment
                    && pending.method_type == key_type
                    && pending.flags == flags
                    && pending.key_len == key.len()
                    && key.starts_with(&pending.key_data);
                if !same {
                    bail!(
                        "a different key upload is pending in {buffer}, \
                         run `close-key-buffer` to discard it first"
                    );
                }
                if self.dry_run {
                    self.step(format!(
                        "a pending upload holds {} of {} bytes, which a dry run does not continue",
                        pending.written(),
                        key.len()
                    ));
                    return Ok(());
                }
                self.step(format!(
                    "resuming at byte {} of {}",
                    pending.written(),
                    key.len()
                ));
                pending.written()
            }
            None => {
                self.step("create_key_buffer".to_string());
                self.execute(
                    &client,
                    ix::create_key_buffer(
                        &payer,
                        &payer,
                        &self.subject,
                        fragment,
                        key_type as u8,
                        flags,
                        key.len() as u32,
                    ),
                )?;
                if self.dry_run {
                    self.step(format!(
                        "dry run: would then send {chunks} write_key_buffer chunks of up to {} \
                         bytes and add_verification_method_from_buffer",
                        ix::KEY_CHUNK_LEN
                    ));
                    return Ok(());
                }
                0
            }
        };

        for (i, chunk) in key[written..].chunks(ix::KEY_CHUNK_LEN).enumerate() {
            let offset = written + i * ix::KEY_CHUNK_LEN;
            self.step(format!(
                "write_key_buffer bytes {offset}..{} of {}",
                offset + chunk.len(),
                key.len()
            ));
            self.execute(
                &client,
                ix::write_key_buffer(&payer, &self.subject, offset as u32, chunk),
            )?;
        }
        self.step("add_verification_method_from_buffer".to_string());
        self.execute(
            &client,
            ix::add_verification_method_from_buffer(&payer, &payer, &self.subject),
        )
    }

    /// The verification method `fragment` as the registry stores it now, if
    /// the account and the method exist.
    fn stored_method(&self, fragment: &str) -> Result<Option<StoredVerificationMethod>> {
        let account = self
            .client()
            .get_account_with_commitment(
                &ix::did_account(&self.subject),
                CommitmentConfig::confirmed(),
            )
            .context("fetching the registry account")?
            .value;
        let Some(account) = account.filter(|account| account.owner == ix::program_id()) else {
            return Ok(None);
        };
        let state = DidAccountState::from_account_data(&account.data)
            .map_err(|e| anyhow!("decoding the registry account: {e}"))?;
        Ok(state.find_verification_method(fragment).cloned())
    }

    /// The key buffer this signer has open for the DID, if any.
    fn pending_upload(
        &self,
        client: &RpcClient,
        buffer: &Pubkey,
    ) -> Result<Option<KeyBufferState>> {
        let account = client
            .get_account_with_commitment(buffer, CommitmentConfig::confirmed())
            .context("fetching the key buffer")?
            .value;
        match account {
            Some(account) if account.owner == ix::program_id() => {
                let state = KeyBufferState::from_account_data(&account.data)
                    .map_err(|e| anyhow!("decoding the key buffer: {e}"))?;
                Ok(Some(state))
            }
            _ => Ok(None),
        }
    }
}

fn network_of(arg: NetworkArg) -> Network {
    match arg {
        NetworkArg::Mainnet => Network::Mainnet,
        NetworkArg::Devnet => Network::Devnet,
        NetworkArg::Testnet => Network::Testnet,
        NetworkArg::Localnet => Network::Localnet,
    }
}

/// The explorer link for a signature. A localnet link carries the RPC
/// endpoint, since the explorer cannot guess it.
fn explorer_url(network: Network, signature: &str, rpc: &str) -> String {
    let base = format!("https://explorer.solana.com/tx/{signature}");
    match network {
        Network::Mainnet => base,
        Network::Devnet => format!("{base}?cluster=devnet"),
        Network::Testnet => format!("{base}?cluster=testnet"),
        Network::Localnet => format!("{base}?cluster=custom&customUrl={}", percent_encode(rpc)),
    }
}

/// Percent-encode everything outside the URL unreserved set.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Read a message from `--message` (UTF-8 text) or `--message-file` (raw
/// bytes).
fn load_message(text: Option<String>, file: Option<&Path>) -> Result<Vec<u8>> {
    match (text, file) {
        (Some(text), None) => Ok(text.into_bytes()),
        (None, Some(path)) => fs::read(path).with_context(|| format!("reading {}", path.display())),
        _ => bail!("pass exactly one of --message or --message-file"),
    }
}

/// Default to the Solana CLI's keypair, `~/.config/solana/id.json`.
fn load_keypair(path: Option<&Path>) -> Result<Keypair> {
    let path: PathBuf = match path {
        Some(path) => path.to_path_buf(),
        None => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow!("HOME is not set, pass --keypair"))?;
            PathBuf::from(home).join(".config/solana/id.json")
        }
    };
    read_keypair_file(&path).map_err(|e| anyhow!("cannot read keypair {}: {e}", path.display()))
}

/// Read key material from `--key` (base58) or `--key-file` (raw bytes) and
/// check its length against the key type before it reaches the program.
fn load_key(key: Option<&str>, key_file: Option<&Path>, key_type: KeyType) -> Result<Vec<u8>> {
    let bytes = match (key, key_file) {
        (Some(key), None) => bs58::decode(key)
            .into_vec()
            .map_err(|e| anyhow!("--key is not valid base58: {e}"))?,
        (None, Some(path)) => {
            fs::read(path).with_context(|| format!("reading {}", path.display()))?
        }
        _ => bail!("pass exactly one of --key or --key-file"),
    };
    let expected = key_type.expected_key_len();
    if bytes.len() != expected {
        bail!(
            "{} keys are {expected} bytes, got {}",
            key_type.on_chain_name(),
            bytes.len()
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use did_bio_core::generative_document;

    use super::*;

    #[test]
    fn localnet_links_carry_the_rpc_endpoint() {
        assert_eq!(
            explorer_url(Network::Localnet, "sig", "http://127.0.0.1:8899"),
            "https://explorer.solana.com/tx/sig?cluster=custom&customUrl=http%3A%2F%2F127.0.0.1%3A8899"
        );
        assert_eq!(
            explorer_url(Network::Devnet, "sig", "https://api.devnet.solana.com"),
            "https://explorer.solana.com/tx/sig?cluster=devnet"
        );
    }

    fn keypair_and_document() -> (Keypair, DidDocument) {
        let keypair = Keypair::new_from_array([7; 32]);
        let did = BioDid::new(Network::Devnet, keypair.pubkey().to_bytes());
        let document = generative_document(&did).expect("a key subject has one");
        (keypair, document)
    }

    #[test]
    fn a_fragment_names_its_method() {
        let (_, document) = keypair_and_document();
        let method = fragment_of(&document, "default").expect("the founding key");
        assert_eq!(method["type"], "Multikey");
        assert!(method["id"].as_str().unwrap().ends_with("#default"));
        assert_eq!(fragment_of(&document, "missing"), None);
    }

    #[test]
    fn the_default_key_signs_under_every_relationship_it_holds() {
        let (keypair, document) = keypair_and_document();
        let signature = keypair.sign_message(b"hello");
        for relationship in [
            RelationshipArg::Authentication,
            RelationshipArg::AssertionMethod,
            RelationshipArg::CapabilityInvocation,
            RelationshipArg::CapabilityDelegation,
        ] {
            verify_signature(
                &document,
                "default",
                relationship,
                b"hello",
                signature.as_ref(),
            )
            .unwrap();
        }
    }

    #[test]
    fn a_wrong_message_or_key_is_rejected() {
        let (keypair, document) = keypair_and_document();
        let signature = keypair.sign_message(b"hello");
        let rejected = |fragment: &str, message: &[u8], signature: &[u8]| {
            verify_signature(
                &document,
                fragment,
                RelationshipArg::Authentication,
                message,
                signature,
            )
            .unwrap_err()
            .to_string()
        };
        assert!(rejected("default", b"other", signature.as_ref()).contains("signature rejected"));
        let stranger = Keypair::new_from_array([8; 32]).sign_message(b"hello");
        assert!(rejected("default", b"hello", stranger.as_ref()).contains("signature rejected"));
        assert!(rejected("missing", b"hello", signature.as_ref()).contains("not found"));
    }
}
