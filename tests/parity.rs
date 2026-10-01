//! Parity between the three crates that must agree on the wire format,
//! which are this resolver, `did-bio-core` and the on-chain program. The
//! program is the source of truth, so when it changes these fail before
//! anything ships.

use bio_did_registry::error::DidError;
use bio_did_registry::{ix as program_ix, state as program, ID};
use bio_did_resolver::cli::{check_flags, check_key};
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
    assert_eq!(ix::INITIALIZE_OWNED, program_ix::INITIALIZE_OWNED);
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
    assert_eq!(core::OWNED_SUBJECT_SEED, program::OWNED_SUBJECT_SEED);
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
    for key_type in [
        KeyType::Ed25519,
        KeyType::X25519,
        KeyType::Secp256k1,
        KeyType::MlDsa87,
    ] {
        assert_eq!(
            Some(key_type.expected_key_len()),
            program::expected_key_len(key_type as u8),
            "{key_type:?}"
        );
    }
}

/// The flag checks the client makes before sending agree with the program's
/// for every key type and every flag value.
#[test]
fn flag_checks_match_the_program() {
    for key_type in [
        KeyType::Ed25519,
        KeyType::X25519,
        KeyType::Secp256k1,
        KeyType::MlDsa87,
    ] {
        for flags in 0..=u16::MAX {
            assert_eq!(
                check_flags(key_type, flags).is_ok(),
                program::validate_vm_flags(key_type as u8, flags).is_ok(),
                "{key_type:?} {flags:#06x}"
            );
        }
    }
}

/// The key checks the client makes before sending agree with the program's
/// on curve points, addresses off the curve, every first byte of a
/// secp256k1 key and the signer exception.
#[test]
fn key_checks_match_the_program() {
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::signature::{Keypair, Signer};
    // An off-curve signer, as a program address signing through CPI is.
    let signer = ix::did_account(&Pubkey::new_unique()).to_bytes();
    let mut keys: Vec<(KeyType, Vec<u8>)> = vec![(KeyType::Ed25519, signer.to_vec())];
    for i in 0..64u8 {
        keys.push((
            KeyType::Ed25519,
            Keypair::new().pubkey().to_bytes().to_vec(),
        ));
        keys.push((
            KeyType::Ed25519,
            ix::did_account(&Pubkey::new_unique()).to_bytes().to_vec(),
        ));
        keys.push((KeyType::Ed25519, vec![i; 32]));
    }
    for first in 0..=u8::MAX {
        let mut key = vec![first];
        key.extend([5u8; 32]);
        keys.push((KeyType::Secp256k1, key));
    }
    keys.push((KeyType::X25519, vec![1; 32]));
    keys.push((KeyType::MlDsa87, vec![1; 2592]));
    for (key_type, key) in &keys {
        for flags in [
            vm_flags::AUTHENTICATION,
            vm_flags::CAPABILITY_INVOCATION | vm_flags::PROTECTED,
        ] {
            let method = program::NewMethod {
                fragment: b"k",
                method_type: *key_type as u8,
                flags,
                key_len: key.len(),
            };
            assert_eq!(
                check_key(*key_type, flags, key, &signer).is_ok(),
                program::check_new_key(&method, key, &signer).is_ok(),
                "{key_type:?} {flags:#06x} {key:?}"
            );
        }
    }
}

/// Every error the program can raise is in the resolver's table under the
/// program's own name and code, and nothing else is.
#[test]
fn program_errors_match_the_program() {
    assert_eq!(DidError::ALL.len(), ix::PROGRAM_ERRORS.len());
    for (variant, (code, name, _)) in DidError::ALL.iter().zip(ix::PROGRAM_ERRORS) {
        assert_eq!(variant.code(), code, "{name}");
        assert_eq!(format!("{variant:?}"), name);
    }
}

/// This crate, the resolver library and the program derive the same owned
/// subject, both for the golden vector the program pins and for arbitrary
/// inputs.
#[test]
fn owned_subjects_match_the_program() {
    let authority = [0x11u8; 32];
    let expected = [
        176, 5, 37, 51, 53, 114, 109, 56, 180, 140, 48, 89, 115, 119, 13, 138, 192, 54, 110, 20,
        205, 247, 212, 197, 39, 52, 9, 159, 203, 10, 250, 28,
    ];
    assert_eq!(program::owned_subject(&authority, 42), expected);
    assert_eq!(did_bio_core::find_owned_subject(&authority, 42).0, expected);
    assert_eq!(
        ix::owned_subject(&solana_sdk::pubkey::Pubkey::new_from_array(authority), 42).to_bytes(),
        expected
    );
    for nonce in [0u64, 1, 7, u64::MAX] {
        let authority = solana_sdk::pubkey::Pubkey::new_unique();
        assert_eq!(
            ix::owned_subject(&authority, nonce).to_bytes(),
            program::owned_subject(&authority.to_bytes(), nonce)
        );
    }
}

#[test]
fn key_buffer_constants_match_the_program() {
    assert_eq!(ix::CREATE_KEY_BUFFER, program_ix::CREATE_KEY_BUFFER);
    assert_eq!(ix::WRITE_KEY_BUFFER, program_ix::WRITE_KEY_BUFFER);
    assert_eq!(
        ix::ADD_VERIFICATION_METHOD_FROM_BUFFER,
        program_ix::ADD_VERIFICATION_METHOD_FROM_BUFFER
    );
    assert_eq!(ix::CLOSE_KEY_BUFFER, program_ix::CLOSE_KEY_BUFFER);
    assert_eq!(core::KEY_BUFFER_SEED, program::KEY_BUFFER_SEED);
    assert_eq!(
        core::KEY_BUFFER_DISCRIMINATOR,
        program::KEY_BUFFER_DISCRIMINATOR
    );
    assert_eq!(core::KEY_BUFFER_HEADER_LEN, program::KEY_BUFFER_HEADER);
}

/// A key buffer image laid out from the program's own offsets decodes
/// through `did-bio-core`.
#[test]
fn key_buffer_layout_roundtrips_through_core() {
    let mut image = vec![0u8; program::KEY_BUFFER_HEADER + 32];
    image[..8].copy_from_slice(&program::KEY_BUFFER_DISCRIMINATOR);
    image[program::KB_OFF_DID_ACCOUNT..program::KB_OFF_DID_ACCOUNT + 32].fill(3);
    image[program::KB_OFF_AUTHORITY..program::KB_OFF_AUTHORITY + 32].fill(4);
    image[program::KB_OFF_BUMP] = 254;
    image[program::KB_OFF_METHOD_TYPE] = program::VM_TYPE_ED25519;
    image[program::KB_OFF_FLAGS..program::KB_OFF_FLAGS + 2]
        .copy_from_slice(&program::VM_FLAG_AUTHENTICATION.to_le_bytes());
    image[program::KB_OFF_KEY_LEN..program::KB_OFF_KEY_LEN + 4]
        .copy_from_slice(&32u32.to_le_bytes());
    image[program::KB_OFF_WRITTEN..program::KB_OFF_WRITTEN + 4]
        .copy_from_slice(&20u32.to_le_bytes());
    image[program::KB_OFF_FRAGMENT_LEN..program::KB_OFF_FRAGMENT_LEN + 4]
        .copy_from_slice(&3u32.to_le_bytes());
    image[program::KB_OFF_FRAGMENT..program::KB_OFF_FRAGMENT + 3].copy_from_slice(b"rot");
    image[program::KB_OFF_KEY..program::KB_OFF_KEY + 20].fill(9);

    let state = core::KeyBufferState::from_account_data(&image).unwrap();
    assert_eq!(state.did_account, [3u8; 32]);
    assert_eq!(state.authority, [4u8; 32]);
    assert_eq!(state.bump, 254);
    assert_eq!(state.method_type, KeyType::Ed25519);
    assert_eq!(state.flags, vm_flags::AUTHENTICATION);
    assert_eq!(state.key_len, 32);
    assert_eq!(state.fragment, "rot");
    assert_eq!(state.written(), 20);
    assert_eq!(state.key_data, vec![9u8; 20]);
    assert!(!state.is_complete());
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
