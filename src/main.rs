//! `bio-did-resolver`: resolve did:bio DIDs and drive the registry program.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use did_bio_core::account::KeyType;
use did_bio_core::{resolve_from_account, BioDid, Network, RawAccount};
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair, Signer};
use solana_sdk::transaction::Transaction;

use bio_did_resolver::cli::{parse_flags, Cli, Command, KeyTypeArg, NetworkArg, WriteOpts};
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
        Command::Resolve { did, url } => resolve(&did, url),
        Command::Init(opts) => {
            let w = Write::new(&opts)?;
            w.send("initialize", ix::initialize(&w.payer(), &w.subject))
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
            let key_type = key_type_of(key_type);
            let key = load_key(key.as_deref(), key_file.as_deref(), key_type)?;
            let vm = ix::VerificationMethod {
                fragment: &fragment,
                key_type: key_type as u8,
                flags: parse_flags(&flags).map_err(|e| anyhow!(e))?,
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
            w.send(
                "set_controllers",
                ix::set_controllers(&w.payer(), &w.payer(), &w.subject, &native, &other),
            )
        }
        Command::Deactivate(opts) => {
            let w = Write::new(&opts)?;
            if !w.yes && !w.dry_run {
                bail!("deactivation is permanent; pass --yes to confirm");
            }
            w.send(
                "deactivate",
                ix::deactivate(&w.payer(), &w.payer(), &w.subject),
            )
        }
    }
}

/// Resolve a DID and print the DID Resolution result as JSON.
///
/// Spec Section 6.2 step 6 permits the generative fallback only for a
/// genuinely absent account: an RPC failure is reported as an error and
/// never silently downgraded to the generative document.
fn resolve(did: &str, url: Option<String>) -> Result<()> {
    let did: BioDid = did.parse().map_err(|e| anyhow!("invalid DID: {e}"))?;
    let url = url.unwrap_or_else(|| did.network.default_rpc_url().to_string());
    let client = RpcClient::new_with_commitment(url, CommitmentConfig::finalized());
    let address = ix::did_account(&Pubkey::new_from_array(did.subject));

    let account = client
        .get_account_with_commitment(&address, CommitmentConfig::finalized())
        .context("fetching the registry account")?
        .value
        .map(|account| RawAccount {
            owner: account.owner.to_bytes(),
            data: account.data,
        });

    let resolution = resolve_from_account(&did, account.as_ref());
    println!("{}", serde_json::to_string_pretty(&resolution)?);
    if let Some(code) = &resolution.resolution_metadata.error {
        bail!("resolution failed: {code}");
    }
    Ok(())
}

/// Everything a write command needs: the target DID, the signing keypair,
/// and where to send the transaction.
struct Write {
    did: BioDid,
    subject: Pubkey,
    keypair: Keypair,
    url: String,
    dry_run: bool,
    yes: bool,
}

impl Write {
    fn new(opts: &WriteOpts) -> Result<Self> {
        let keypair = load_keypair(opts.keypair.as_deref())?;
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
        })
    }

    /// The keypair pays and, for updates, signs as the update authority.
    fn payer(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    fn send(&self, action: &str, instruction: Instruction) -> Result<()> {
        if self.did.network == Network::Mainnet && !self.dry_run && !self.yes {
            bail!("refusing to send `{action}` to mainnet without --yes");
        }
        let client =
            RpcClient::new_with_commitment(self.url.clone(), CommitmentConfig::confirmed());
        let blockhash = client
            .get_latest_blockhash()
            .context("fetching a recent blockhash")?;
        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            blockhash,
        );

        println!("{action}");
        println!("  did      {}", self.did);
        println!("  account  {}", ix::did_account(&self.subject));
        println!("  signer   {}", self.keypair.pubkey());
        println!("  rpc      {}", self.url);

        if self.dry_run {
            let result = client
                .simulate_transaction(&tx)
                .context("simulating the transaction")?
                .value;
            for line in result.logs.unwrap_or_default() {
                println!("  {line}");
            }
            if let Some(units) = result.units_consumed {
                println!("  compute units {units}");
            }
            return match result.err {
                Some(err) => Err(anyhow!("simulation failed: {err}")),
                None => Ok(()),
            };
        }

        let signature = client
            .send_and_confirm_transaction(&tx)
            .context("sending the transaction")?;
        println!("  signature {signature}");
        println!(
            "  {}",
            explorer_url(self.did.network, &signature.to_string())
        );
        Ok(())
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

fn key_type_of(arg: KeyTypeArg) -> KeyType {
    match arg {
        KeyTypeArg::Ed25519 => KeyType::Ed25519,
        KeyTypeArg::X25519 => KeyType::X25519,
        KeyTypeArg::Secp256k1 => KeyType::Secp256k1,
        KeyTypeArg::MlDsa87 => KeyType::MlDsa87,
    }
}

fn explorer_url(network: Network, signature: &str) -> String {
    let base = format!("https://explorer.solana.com/tx/{signature}");
    match network {
        Network::Mainnet => base,
        Network::Devnet => format!("{base}?cluster=devnet"),
        Network::Testnet => format!("{base}?cluster=testnet"),
        Network::Localnet => format!("{base}?cluster=custom"),
    }
}

/// Default to the Solana CLI's keypair, `~/.config/solana/id.json`.
fn load_keypair(path: Option<&Path>) -> Result<Keypair> {
    let path: PathBuf = match path {
        Some(path) => path.to_path_buf(),
        None => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow!("HOME is not set; pass --keypair"))?;
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
