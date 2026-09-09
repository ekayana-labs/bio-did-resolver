//! The wire format the resolver produces, pinned independently of the
//! program: discriminators recomputed from instruction names, borsh
//! argument layout, and account metas.

use std::str::FromStr;

use bio_did_resolver::ix::{self, Service, VerificationMethod};
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;

fn discriminator(name: &str) -> [u8; 8] {
    let digest = Sha256::digest(format!("global:{name}").as_bytes());
    digest[..8].try_into().unwrap()
}

fn str_bytes(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

#[test]
fn discriminators_are_sha256_of_instruction_names() {
    for (constant, name) in [
        (ix::INITIALIZE, "initialize"),
        (ix::ADD_VERIFICATION_METHOD, "add_verification_method"),
        (ix::REMOVE_VERIFICATION_METHOD, "remove_verification_method"),
        (
            ix::SET_VERIFICATION_METHOD_FLAGS,
            "set_verification_method_flags",
        ),
        (ix::ADD_SERVICE, "add_service"),
        (ix::REMOVE_SERVICE, "remove_service"),
        (ix::SET_CONTROLLERS, "set_controllers"),
        (ix::DEACTIVATE, "deactivate"),
        (ix::CREATE_KEY_BUFFER, "create_key_buffer"),
        (ix::WRITE_KEY_BUFFER, "write_key_buffer"),
        (
            ix::ADD_VERIFICATION_METHOD_FROM_BUFFER,
            "add_verification_method_from_buffer",
        ),
        (ix::CLOSE_KEY_BUFFER, "close_key_buffer"),
    ] {
        assert_eq!(constant, discriminator(name), "{name}");
    }
}

#[test]
fn key_buffer_matches_the_sdk_derivation() {
    let (subject, authority) = (Pubkey::new_unique(), Pubkey::new_unique());
    let (expected, _bump) = Pubkey::find_program_address(
        &[
            b"bio-did-key",
            ix::did_account(&subject).as_ref(),
            authority.as_ref(),
        ],
        &ix::program_id(),
    );
    assert_eq!(ix::key_buffer(&subject, &authority), expected);

    // The vector did-bio-core pins for the spec example subject.
    let subject = Pubkey::from_str("2T6zLFvMx7NJac5qQtiKTaPhMwHLkwKETWjUK1yKv4tc").unwrap();
    assert_eq!(
        ix::key_buffer(&subject, &subject).to_string(),
        "2DLfor8ZYBiiGt6MMDhPB6tnDUKejkkotHfMG6xFDcnh"
    );
}

#[test]
fn key_buffer_layouts() {
    let (payer, authority, subject) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let buffer = ix::key_buffer(&subject, &authority);

    let create = ix::create_key_buffer(&payer, &authority, &subject, "pq", 3, 0b10, 2592);
    let mut data = ix::CREATE_KEY_BUFFER.to_vec();
    data.extend(str_bytes("pq"));
    data.push(3);
    data.extend_from_slice(&0b10u16.to_le_bytes());
    data.extend_from_slice(&2592u32.to_le_bytes());
    assert_eq!(create.data, data);
    let metas = &create.accounts;
    assert_eq!(metas.len(), 5);
    assert!(metas[0].is_signer && metas[0].is_writable && metas[0].pubkey == payer);
    assert!(metas[1].is_signer && !metas[1].is_writable && metas[1].pubkey == authority);
    assert!(
        !metas[2].is_signer && metas[2].is_writable && metas[2].pubkey == ix::did_account(&subject)
    );
    assert!(!metas[3].is_signer && metas[3].is_writable && metas[3].pubkey == buffer);
    assert!(!metas[4].is_signer && !metas[4].is_writable && metas[4].pubkey == ix::SYSTEM_PROGRAM);

    let write = ix::write_key_buffer(&authority, &subject, 900, &[7, 7, 7]);
    let mut data = ix::WRITE_KEY_BUFFER.to_vec();
    data.extend_from_slice(&900u32.to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&[7, 7, 7]);
    assert_eq!(write.data, data);
    let metas = &write.accounts;
    assert_eq!(metas.len(), 2);
    assert!(metas[0].is_signer && !metas[0].is_writable && metas[0].pubkey == authority);
    assert!(!metas[1].is_signer && metas[1].is_writable && metas[1].pubkey == buffer);

    let finish = ix::add_verification_method_from_buffer(&payer, &authority, &subject);
    assert_eq!(finish.data, ix::ADD_VERIFICATION_METHOD_FROM_BUFFER);
    assert_eq!(finish.accounts, create.accounts);

    let close = ix::close_key_buffer(&payer, &authority, &subject);
    assert_eq!(close.data, ix::CLOSE_KEY_BUFFER);
    let metas = &close.accounts;
    assert_eq!(metas.len(), 3);
    assert!(metas[0].is_signer && metas[0].is_writable && metas[0].pubkey == payer);
    assert!(metas[1].is_signer && !metas[1].is_writable && metas[1].pubkey == authority);
    assert!(!metas[2].is_signer && metas[2].is_writable && metas[2].pubkey == buffer);
}

/// The two size constants against the real transaction encoding.
#[test]
fn chunk_and_inline_limits_fit_in_a_packet() {
    use solana_sdk::instruction::Instruction;
    use solana_sdk::message::Message;
    const PACKET_DATA_SIZE: usize = 1232;
    let size = |ix: Instruction, payer: &Pubkey, signers: usize| {
        1 + 64 * signers + Message::new(&[ix], Some(payer)).serialize().len()
    };
    let (payer, authority, subject) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );

    // A full chunk, with a fee payer separate from the authority.
    let chunk = vec![7u8; ix::KEY_CHUNK_LEN];
    let write = ix::write_key_buffer(&authority, &subject, 0, &chunk);
    assert!(size(write, &payer, 2) <= PACKET_DATA_SIZE);

    // The largest inline key with the longest fragment and a separate payer.
    let key = vec![7u8; ix::MAX_INLINE_KEY_LEN];
    let fragment = "f".repeat(32);
    let vm = VerificationMethod {
        fragment: &fragment,
        key_type: 3,
        flags: 0b10,
        key: &key,
    };
    let add = ix::add_verification_method(&payer, &authority, &subject, &vm);
    assert!(size(add, &payer, 2) <= PACKET_DATA_SIZE);

    // An ML-DSA-87 key inline does not fit, even with a single signer.
    let key = vec![7u8; 2592];
    let vm = VerificationMethod {
        fragment: "pq",
        key_type: 3,
        flags: 0b10,
        key: &key,
    };
    let add = ix::add_verification_method(&authority, &authority, &subject, &vm);
    assert!(size(add, &authority, 1) > PACKET_DATA_SIZE);
}

#[test]
fn system_program_is_the_zero_key() {
    assert_eq!(
        ix::SYSTEM_PROGRAM.to_string(),
        "11111111111111111111111111111111"
    );
}

#[test]
fn registry_account_matches_the_sdk_derivation() {
    let subject = Pubkey::new_unique();
    let (expected, _bump) =
        Pubkey::find_program_address(&[b"bio-did", subject.as_ref()], &ix::program_id());
    assert_eq!(ix::did_account(&subject), expected);
}

#[test]
fn initialize_layout() {
    let payer = Pubkey::new_unique();
    let subject = Pubkey::new_unique();
    let instruction = ix::initialize(&payer, &subject);

    let mut data = ix::INITIALIZE.to_vec();
    data.extend_from_slice(subject.as_ref());
    assert_eq!(instruction.data, data);
    assert_eq!(instruction.program_id, ix::program_id());

    let metas = &instruction.accounts;
    assert_eq!(metas.len(), 3);
    assert!(metas[0].is_signer && metas[0].is_writable && metas[0].pubkey == payer);
    assert!(
        !metas[1].is_signer && metas[1].is_writable && metas[1].pubkey == ix::did_account(&subject)
    );
    assert!(!metas[2].is_signer && !metas[2].is_writable && metas[2].pubkey == ix::SYSTEM_PROGRAM);
}

#[test]
fn add_verification_method_layout() {
    let (payer, authority, subject) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let key = [7u8; 2592];
    let vm = VerificationMethod {
        fragment: "pq",
        key_type: 3,
        flags: 0b10,
        key: &key,
    };
    let instruction = ix::add_verification_method(&payer, &authority, &subject, &vm);

    let mut data = ix::ADD_VERIFICATION_METHOD.to_vec();
    data.extend(str_bytes("pq"));
    data.push(3);
    data.extend_from_slice(&0b10u16.to_le_bytes());
    data.extend_from_slice(&2592u32.to_le_bytes());
    data.extend_from_slice(&key);
    assert_eq!(instruction.data, data);

    let metas = &instruction.accounts;
    assert_eq!(metas.len(), 4);
    assert!(metas[0].is_signer && metas[0].is_writable && metas[0].pubkey == payer);
    assert!(metas[1].is_signer && !metas[1].is_writable && metas[1].pubkey == authority);
    assert!(
        !metas[2].is_signer && metas[2].is_writable && metas[2].pubkey == ix::did_account(&subject)
    );
    assert!(!metas[3].is_signer && !metas[3].is_writable && metas[3].pubkey == ix::SYSTEM_PROGRAM);
}

#[test]
fn set_flags_needs_no_payer() {
    let (authority, subject) = (Pubkey::new_unique(), Pubkey::new_unique());
    let instruction = ix::set_verification_method_flags(&authority, &subject, "default", 0x011F);

    let mut data = ix::SET_VERIFICATION_METHOD_FLAGS.to_vec();
    data.extend(str_bytes("default"));
    data.extend_from_slice(&0x011Fu16.to_le_bytes());
    assert_eq!(instruction.data, data);

    let metas = &instruction.accounts;
    assert_eq!(metas.len(), 2);
    assert!(metas[0].is_signer && !metas[0].is_writable && metas[0].pubkey == authority);
    assert!(!metas[1].is_signer && metas[1].is_writable);
}

#[test]
fn service_and_fragment_layouts() {
    let (payer, authority, subject) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let service = Service {
        fragment: "metadata",
        service_type: "BioMetadata",
        endpoint: "ipfs://bafy",
    };
    let add = ix::add_service(&payer, &authority, &subject, &service);
    let mut data = ix::ADD_SERVICE.to_vec();
    data.extend(str_bytes("metadata"));
    data.extend(str_bytes("BioMetadata"));
    data.extend(str_bytes("ipfs://bafy"));
    assert_eq!(add.data, data);

    let remove = ix::remove_service(&payer, &authority, &subject, "metadata");
    let mut data = ix::REMOVE_SERVICE.to_vec();
    data.extend(str_bytes("metadata"));
    assert_eq!(remove.data, data);

    let remove_vm = ix::remove_verification_method(&payer, &authority, &subject, "pq");
    let mut data = ix::REMOVE_VERIFICATION_METHOD.to_vec();
    data.extend(str_bytes("pq"));
    assert_eq!(remove_vm.data, data);
}

#[test]
fn set_controllers_layout() {
    let (payer, authority, subject) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let lab = Pubkey::new_unique();
    let instruction = ix::set_controllers(
        &payer,
        &authority,
        &subject,
        &[lab],
        &["did:web:lab.example.org".to_string()],
    );

    let mut data = ix::SET_CONTROLLERS.to_vec();
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(lab.as_ref());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend(str_bytes("did:web:lab.example.org"));
    assert_eq!(instruction.data, data);
}

#[test]
fn deactivate_carries_only_the_discriminator() {
    let (payer, authority, subject) = (
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    );
    let instruction = ix::deactivate(&payer, &authority, &subject);
    assert_eq!(instruction.data, ix::DEACTIVATE);
    assert_eq!(instruction.accounts.len(), 4);
}
