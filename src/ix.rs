//! Instruction encoding for the did:bio registry program.
//!
//! The program ships no client side builders, so the wire format lives
//! here: an 8 byte discriminator, `sha256("global:<name>")[..8]`, followed
//! by the borsh encoded arguments. The tests recompute every discriminator
//! from its name and pin the encoded bytes.

use did_bio_core::account::PROGRAM_ID;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;

/// The system program, `11111111111111111111111111111111`.
pub const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([0; 32]);

pub const INITIALIZE: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];
pub const ADD_VERIFICATION_METHOD: [u8; 8] = [213, 200, 190, 61, 28, 104, 245, 25];
pub const REMOVE_VERIFICATION_METHOD: [u8; 8] = [33, 238, 66, 183, 62, 210, 133, 150];
pub const SET_VERIFICATION_METHOD_FLAGS: [u8; 8] = [16, 188, 26, 223, 241, 131, 192, 223];
pub const ADD_SERVICE: [u8; 8] = [133, 207, 106, 32, 91, 111, 153, 30];
pub const REMOVE_SERVICE: [u8; 8] = [19, 102, 8, 231, 40, 141, 9, 110];
pub const SET_CONTROLLERS: [u8; 8] = [65, 40, 24, 8, 30, 81, 20, 179];
pub const DEACTIVATE: [u8; 8] = [44, 112, 33, 172, 113, 28, 142, 13];

/// The registry program ID.
pub fn program_id() -> Pubkey {
    Pubkey::new_from_array(PROGRAM_ID)
}

/// The registry account for a subject key: `["bio-did", subject]`.
pub fn did_account(subject: &Pubkey) -> Pubkey {
    let (address, _bump) = did_bio_core::find_did_account_address(&subject.to_bytes());
    Pubkey::new_from_array(address)
}

/// A verification method as the program receives it.
pub struct VerificationMethod<'a> {
    pub fragment: &'a str,
    /// On chain key type tag; see `did_bio_core::account::KeyType`.
    pub key_type: u8,
    pub flags: u16,
    pub key: &'a [u8],
}

/// A service as the program receives it.
pub struct Service<'a> {
    pub fragment: &'a str,
    pub service_type: &'a str,
    pub endpoint: &'a str,
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_bytes(buf, s.as_bytes());
}

fn put_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn instruction(data: Vec<u8>, accounts: Vec<AccountMeta>) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts,
        data,
    }
}

/// Accounts of every instruction that may resize the registry account:
/// the payer funds growth and receives shrink refunds.
fn update_accounts(payer: &Pubkey, authority: &Pubkey, subject: &Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(*payer, true),
        AccountMeta::new_readonly(*authority, true),
        AccountMeta::new(did_account(subject), false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
    ]
}

/// Create the registry account holding the generative document.
/// Permissionless: the payer need not be the subject.
pub fn initialize(payer: &Pubkey, subject: &Pubkey) -> Instruction {
    let mut data = INITIALIZE.to_vec();
    data.extend_from_slice(subject.as_ref());
    instruction(
        data,
        vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(did_account(subject), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
        ],
    )
}

pub fn add_verification_method(
    payer: &Pubkey,
    authority: &Pubkey,
    subject: &Pubkey,
    vm: &VerificationMethod<'_>,
) -> Instruction {
    let mut data = ADD_VERIFICATION_METHOD.to_vec();
    put_str(&mut data, vm.fragment);
    data.push(vm.key_type);
    data.extend_from_slice(&vm.flags.to_le_bytes());
    put_bytes(&mut data, vm.key);
    instruction(data, update_accounts(payer, authority, subject))
}

pub fn remove_verification_method(
    payer: &Pubkey,
    authority: &Pubkey,
    subject: &Pubkey,
    fragment: &str,
) -> Instruction {
    let mut data = REMOVE_VERIFICATION_METHOD.to_vec();
    put_str(&mut data, fragment);
    instruction(data, update_accounts(payer, authority, subject))
}

/// The one update that never resizes, so it needs no payer.
pub fn set_verification_method_flags(
    authority: &Pubkey,
    subject: &Pubkey,
    fragment: &str,
    flags: u16,
) -> Instruction {
    let mut data = SET_VERIFICATION_METHOD_FLAGS.to_vec();
    put_str(&mut data, fragment);
    data.extend_from_slice(&flags.to_le_bytes());
    instruction(
        data,
        vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(did_account(subject), false),
        ],
    )
}

pub fn add_service(
    payer: &Pubkey,
    authority: &Pubkey,
    subject: &Pubkey,
    service: &Service<'_>,
) -> Instruction {
    let mut data = ADD_SERVICE.to_vec();
    put_str(&mut data, service.fragment);
    put_str(&mut data, service.service_type);
    put_str(&mut data, service.endpoint);
    instruction(data, update_accounts(payer, authority, subject))
}

pub fn remove_service(
    payer: &Pubkey,
    authority: &Pubkey,
    subject: &Pubkey,
    fragment: &str,
) -> Instruction {
    let mut data = REMOVE_SERVICE.to_vec();
    put_str(&mut data, fragment);
    instruction(data, update_accounts(payer, authority, subject))
}

/// Replace both controller sets at once.
pub fn set_controllers(
    payer: &Pubkey,
    authority: &Pubkey,
    subject: &Pubkey,
    native: &[Pubkey],
    other: &[String],
) -> Instruction {
    let mut data = SET_CONTROLLERS.to_vec();
    data.extend_from_slice(&(native.len() as u32).to_le_bytes());
    for key in native {
        data.extend_from_slice(key.as_ref());
    }
    data.extend_from_slice(&(other.len() as u32).to_le_bytes());
    for did in other {
        put_str(&mut data, did);
    }
    instruction(data, update_accounts(payer, authority, subject))
}

/// Permanently deactivate the DID. There is no instruction that undoes it.
pub fn deactivate(payer: &Pubkey, authority: &Pubkey, subject: &Pubkey) -> Instruction {
    instruction(
        DEACTIVATE.to_vec(),
        update_accounts(payer, authority, subject),
    )
}
