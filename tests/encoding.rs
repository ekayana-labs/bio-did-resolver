//! The wire format the resolver produces, pinned independently of the
//! program: discriminators recomputed from instruction names, borsh
//! argument layout, and account metas.

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
    ] {
        assert_eq!(constant, discriminator(name), "{name}");
    }
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
