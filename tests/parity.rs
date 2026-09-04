//! Parity between the three crates that must agree on the wire format:
//! this resolver, `did-bio-core`, and the on-chain program. The program
//! is the source of truth; if it changes, these fail before anything ships.

use bio_did_registry::{ix as program_ix, state as program, ID};
use bio_did_resolver::ix;
use did_bio_core::account::{self as core, vm_flags, KeyType};

#[test]
fn program_id_matches() {
    let id: &[u8] = ID.as_ref();
    assert_eq!(id, core::PROGRAM_ID);
    assert_eq!(ix::program_id().to_bytes(), core::PROGRAM_ID);
}

#[test]
fn discriminators_match_the_program() {
    assert_eq!(ix::INITIALIZE, program_ix::INITIALIZE);
    assert_eq!(
        ix::ADD_VERIFICATION_METHOD,
        program_ix::ADD_VERIFICATION_METHOD
    );
    assert_eq!(
        ix::REMOVE_VERIFICATION_METHOD,
        program_ix::REMOVE_VERIFICATION_METHOD
    );
    assert_eq!(
        ix::SET_VERIFICATION_METHOD_FLAGS,
        program_ix::SET_VERIFICATION_METHOD_FLAGS
    );
    assert_eq!(ix::ADD_SERVICE, program_ix::ADD_SERVICE);
    assert_eq!(ix::REMOVE_SERVICE, program_ix::REMOVE_SERVICE);
    assert_eq!(ix::SET_CONTROLLERS, program_ix::SET_CONTROLLERS);
    assert_eq!(ix::DEACTIVATE, program_ix::DEACTIVATE);
}

#[test]
fn account_constants_match_the_program() {
    assert_eq!(core::ACCOUNT_DISCRIMINATOR, program::ACCOUNT_DISCRIMINATOR);
    assert_eq!(core::DID_SEED, program::DID_SEED);
    assert_eq!(core::DEFAULT_FRAGMENT.as_bytes(), program::DEFAULT_FRAGMENT);
    assert_eq!(
        core::MAX_VERIFICATION_METHODS,
        program::MAX_VERIFICATION_METHODS
    );
    assert_eq!(core::MAX_SERVICES, program::MAX_SERVICES);
    assert_eq!(
        core::MAX_NATIVE_CONTROLLERS,
        program::MAX_NATIVE_CONTROLLERS
    );
    assert_eq!(core::MAX_OTHER_CONTROLLERS, program::MAX_OTHER_CONTROLLERS);
    assert_eq!(core::MAX_FRAGMENT_LEN, program::MAX_FRAGMENT_LEN);
    assert_eq!(core::MAX_SERVICE_TYPE_LEN, program::MAX_SERVICE_TYPE_LEN);
    assert_eq!(core::MAX_ENDPOINT_LEN, program::MAX_ENDPOINT_LEN);
    assert_eq!(core::MAX_CONTROLLER_LEN, program::MAX_CONTROLLER_LEN);
    assert_eq!(core::MAX_KEY_DATA_LEN, program::MAX_KEY_DATA_LEN);
}

#[test]
fn flags_and_key_types_match_the_program() {
    assert_eq!(vm_flags::AUTHENTICATION, program::VM_FLAG_AUTHENTICATION);
    assert_eq!(vm_flags::ASSERTION, program::VM_FLAG_ASSERTION);
    assert_eq!(vm_flags::KEY_AGREEMENT, program::VM_FLAG_KEY_AGREEMENT);
    assert_eq!(
        vm_flags::CAPABILITY_INVOCATION,
        program::VM_FLAG_CAPABILITY_INVOCATION
    );
    assert_eq!(
        vm_flags::CAPABILITY_DELEGATION,
        program::VM_FLAG_CAPABILITY_DELEGATION
    );
    assert_eq!(vm_flags::PROTECTED, program::VM_FLAG_PROTECTED);
    assert_eq!(vm_flags::RELATIONSHIP_MASK, program::VM_RELATIONSHIP_MASK);
    assert_eq!(vm_flags::VALID_MASK, program::VM_VALID_MASK);
    assert_eq!(vm_flags::DEFAULT, program::VM_FLAGS_DEFAULT);

    assert_eq!(KeyType::Ed25519 as u8, program::VM_TYPE_ED25519);
    assert_eq!(KeyType::X25519 as u8, program::VM_TYPE_X25519);
    assert_eq!(KeyType::Secp256k1 as u8, program::VM_TYPE_SECP256K1);
    assert_eq!(KeyType::MlDsa87 as u8, program::VM_TYPE_DILITHIUM5);
    assert_eq!(
        KeyType::MlDsa87.expected_key_len(),
        program::MAX_KEY_DATA_LEN
    );
}

/// A generative account image built from the program's own layout
/// constants decodes through `did-bio-core` to the generative state.
#[test]
fn generative_layout_roundtrips_through_core() {
    let subject = [21u8; 32];
    let mut image = Vec::with_capacity(program::INITIAL_SPACE);
    image.extend_from_slice(&program::ACCOUNT_DISCRIMINATOR);
    image.extend_from_slice(&1u64.to_le_bytes());
    image.push(254);
    image.extend_from_slice(&subject);
    image.push(0);
    image.extend_from_slice(&0i64.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&1u32.to_le_bytes());
    image.extend_from_slice(&(program::DEFAULT_FRAGMENT.len() as u32).to_le_bytes());
    image.extend_from_slice(program::DEFAULT_FRAGMENT);
    image.push(program::VM_TYPE_ED25519);
    image.extend_from_slice(&program::VM_FLAGS_DEFAULT.to_le_bytes());
    image.extend_from_slice(&32u32.to_le_bytes());
    image.extend_from_slice(&subject);
    image.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(image.len(), program::INITIAL_SPACE);

    let state = core::DidAccountState::from_account_data(&image).unwrap();
    let mut generative = core::DidAccountState::generative(subject);
    generative.version = 1;
    generative.bump = 254;
    assert_eq!(state, generative);
    assert_eq!(program::TOMBSTONE_SPACE, program::BASE_SPACE);
}
