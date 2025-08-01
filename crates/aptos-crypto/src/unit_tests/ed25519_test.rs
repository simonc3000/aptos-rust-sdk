// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::redundant_clone)] // Required to work around prop_assert_eq! limitations

use crate as aptos_crypto;
use crate::{
    ed25519::{
        Ed25519PrivateKey, Ed25519PublicKey, Ed25519Signature, ED25519_PRIVATE_KEY_LENGTH,
        ED25519_PUBLIC_KEY_LENGTH, ED25519_SIGNATURE_LENGTH,
    },
    test_utils::{
        random_serializable_struct, small_order_pk_with_adversarial_message,
        uniform_keypair_strategy,
    },
    traits::*,
};
use aptos_crypto_derive::{BCSCryptoHash, CryptoHasher};
use core::{
    convert::TryFrom,
    ops::{Add, Index, IndexMut, Mul, Neg},
};
use curve25519_dalek::{
    constants::ED25519_BASEPOINT_POINT,
    edwards::{CompressedEdwardsY, EdwardsPoint},
    scalar::Scalar,
};
use digest::Digest;
use ed25519_dalek::ed25519::signature::Verifier as _;
use proptest::{collection::vec, prelude::*};
use serde::{Deserialize, Serialize};
use sha2::Sha512;

#[derive(CryptoHasher, BCSCryptoHash, Serialize, Deserialize)]
struct CryptoHashable(pub usize);

// Takes a point in eight_torsion and finds its order
fn eight_torsion_order(ep: EdwardsPoint) -> usize {
    let mut pt = ep;
    let mut ord = 1;
    for _i in 0..8 {
        if pt == EdwardsPoint::default() {
            break;
        } else {
            pt = pt.add(ep);
            ord += 1;
        }
    }
    ord
}

proptest! {
    #[test]
    fn verify_canonicity_torsion(scalar in any::<[u8;32]>(), idx in 0usize..8usize){
        let s = Scalar::from_bytes_mod_order(scalar);
        let s_b = ED25519_BASEPOINT_POINT.mul(s);
        let torsion_component = CompressedEdwardsY(EIGHT_TORSION[idx]).decompress().unwrap();
        let torsioned = s_b.add(torsion_component);
        let torsioned_bytes = torsioned.compress().to_bytes();
        let deserialized = CompressedEdwardsY(torsioned_bytes).decompress().unwrap();
        prop_assert_eq!(deserialized, torsioned);
    }


    #[test]
    fn verify_mul_torsion(idx in 0usize..8usize){
        let torsion_component = CompressedEdwardsY(EIGHT_TORSION[idx]).decompress().unwrap();
        let mut order_bytes = [0u8;32];
        order_bytes[..8].copy_from_slice(&(eight_torsion_order(torsion_component)).to_le_bytes());
        let torsion_order = Scalar::from_bits(order_bytes);

        prop_assert_eq!(torsion_component.mul(torsion_order), EdwardsPoint::default());
    }

    // In this test we demonstrate a signature that's not message-bound by only
    // modifying the public key and the R component, under a pathological yet
    // admissible s < l value for the signature
    #[test]
    fn verify_sig_malleable_torsion(keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>(), idx in 0usize..8usize){
        let message = b"hello_world";
        ///////////////////////////////////
        // baseline signature components //
        ///////////////////////////////////
        let pub_key_bytes = keypair.public_key.to_bytes();
        let priv_key_bytes = keypair.private_key.to_bytes();
        let pub_key = ed25519_dalek::PublicKey::from_bytes(&pub_key_bytes).unwrap();
        let secret_key = ed25519_dalek::SecretKey::from_bytes(&priv_key_bytes).unwrap();
        let priv_key = ed25519_dalek::ExpandedSecretKey::from(&secret_key);
        let signature = priv_key.sign(&message[..], &pub_key);
        prop_assert!(pub_key.verify(&message[..], &signature).is_ok());

        let torsion_component = CompressedEdwardsY(EIGHT_TORSION[idx]).decompress().unwrap();

        let mut r_bits = [0u8; 32];
        r_bits.copy_from_slice(&signature.to_bytes()[..32]);

        let r_point = CompressedEdwardsY(r_bits).decompress().unwrap();
        let mixed_r_point = r_point.add(torsion_component);
        prop_assert_eq!(r_point.mul_by_cofactor(), mixed_r_point.mul_by_cofactor());

        let pub_point = CompressedEdwardsY(pub_key_bytes).decompress().unwrap();
        let mixed_pub_point = pub_point.add(torsion_component);
        prop_assert_eq!(pub_point.mul_by_cofactor(), mixed_pub_point.mul_by_cofactor());

        //////////////////////////
        // Compute k = H(R∥A∥m) //
        //////////////////////////
        let mut h: Sha512 = Sha512::default();
        h.update(mixed_r_point.compress().to_bytes());
        h.update(mixed_pub_point.compress().to_bytes());
        h.update(message);
        // curve25519_dalek is stuck on an old digest version, so we can't do
        // Scalar::from_hash
        let mut output = [0u8; 64];
        output.copy_from_slice(h.finalize().as_slice());
        let k = Scalar::from_bytes_mod_order_wide(&output);

        //////////////////////////////////////////////////////////////
        // obtain the original r s.t. R = r B, to solve for s later //
        //////////////////////////////////////////////////////////////
        let mut expanded_priv_key = [0u8; 64];
        let mut h: Sha512 = Sha512::default();
        h.update(priv_key_bytes);
        expanded_priv_key.copy_from_slice(h.finalize().as_slice());

        let nonce = &expanded_priv_key[32..];
        let mut h: Sha512 = Sha512::default();
        h.update(nonce);
        h.update(message);
        // curve25519_dalek is stuck on an old digest version, so we can't do
        // Scalar::from_hash
        let mut output = [0u8; 64];
        output.copy_from_slice(h.finalize().as_slice());
        let original_r = Scalar::from_bytes_mod_order_wide(&output);

        // check r_point = original_r * basepoint
        prop_assert_eq!(ED25519_BASEPOINT_POINT.mul(original_r), r_point);

        //////////////////////////////////////////
        // obtain the original a s.t. a * B = A //
        //////////////////////////////////////////
        let mut key_bytes = [0u8;32];
        key_bytes.copy_from_slice(&expanded_priv_key[..32]);
        key_bytes[0] &= 248;
        key_bytes[31] &= 127;
        key_bytes[31] |= 64;
        let priv_scalar = Scalar::from_bits(key_bytes);
        // check pub_point = priv_scalar * basepoint
        prop_assert_eq!(ED25519_BASEPOINT_POINT.mul(priv_scalar), pub_point);

        //////////////////////////
        // s = r + k a as usual //
        //////////////////////////
        let s = k * priv_scalar + original_r;
        prop_assert!(s.is_canonical());

        /////////////////////////////////////////////////////////////////////////////////
        // Check the cofactored equation (modulo 8) before conversion to dalek formats //
        /////////////////////////////////////////////////////////////////////////////////
        let mut eight_scalar_bytes = [0u8;32];
        eight_scalar_bytes[..8].copy_from_slice(&(8_usize).to_le_bytes());
        let eight_scalar = Scalar::from_bits(eight_scalar_bytes);

        let r_candidate_point = EdwardsPoint::vartime_double_scalar_mul_basepoint(&k, &(mixed_pub_point.neg().mul_by_cofactor()), &(s * eight_scalar));
        prop_assert_eq!(mixed_r_point.mul_by_cofactor(), r_candidate_point);

        ///////////////////////////////////////////////////////////
        // convert byte strings in dalek terms and do API checks //
        ///////////////////////////////////////////////////////////
        let mixed_pub_key = ed25519_dalek::PublicKey::from_bytes(&mixed_pub_point.compress().to_bytes()).unwrap();
        // check we would not have caught this mixed order point on PublicKey deserialization
        prop_assert!(Ed25519PublicKey::try_from(&mixed_pub_point.compress().to_bytes()[..]).is_ok());

        let mixed_signature_bits : Vec<u8> = [mixed_r_point.compress().to_bytes(), s.to_bytes()].concat();
        // this will error if we don't have 0 ≤ s < l
        let mixed_signature = ed25519_dalek::Signature::from_bytes(&mixed_signature_bits).unwrap();

        // Check, however, that dalek is doing the raw equation check sB = R + kA
        let permissive_passes = mixed_pub_key.verify(&message[..], &mixed_signature).is_ok();
        let strict_passes = mixed_pub_key.verify_strict(&message[..], &mixed_signature).is_ok();

        let torsion_order = eight_torsion_order(torsion_component);
        let torsion_components_cancel = torsion_component + k * torsion_component == EdwardsPoint::default();

        prop_assert!(!permissive_passes || (torsion_order == 1) || torsion_components_cancel,
                     "bad verification_state permissive passes {} strict passes {} mixed_order {:?} torsion_components_cancel {:?}",
                     permissive_passes,
                     strict_passes,
                     torsion_order,
                     torsion_components_cancel
        );
    }

    // In this test we demonstrate a signature that's transformable by only
    // modifying the public key and the R component, under a pathological yet
    // admissible s < l value for the signature. It shows the difference
    // between `verify` and `verify_strict` in ed25519-dalek
    #[test]
    fn verify_sig_strict_torsion(idx in 0usize..8usize){
        let message = b"hello_world";

        // Dalek only performs an order check, so this is allowed
        let bad_scalar = Scalar::zero();

        let bad_component_1 = curve25519_dalek::constants::EIGHT_TORSION[idx];
        let bad_component_2 = bad_component_1.neg();

        // compute bad_pub_key, bad_signature
        let bad_pub_key_point = bad_component_1; // we need this to cancel the hashed component of the verification equation

        // we pick an evil R component
        let bad_sig_point = bad_component_2;

        let bad_key = ed25519_dalek::PublicKey::from_bytes(&bad_pub_key_point.compress().to_bytes()).unwrap();
        // This assertion passes because Ed25519PublicKey::TryFrom<&[u8]> no longer checks for small subgroup membership
        prop_assert!(Ed25519PublicKey::try_from(&bad_pub_key_point.compress().to_bytes()[..]).is_ok());

        let bad_signature = ed25519_dalek::Signature::from_bytes(&[
            &bad_sig_point.compress().to_bytes()[..],
            &bad_scalar.to_bytes()[..]
        ].concat()).unwrap();

        // Seek k = H(R, A, M) ≡ 1 [8] so that sB - kA = R <=> -kA = -A <=> k mod order(A) = 0
        prop_assume!(bad_key.verify(&message[..], &bad_signature).is_ok());
        prop_assert!(bad_key.verify_strict(&message[..], &bad_signature).is_err());
    }

    #[test]
    fn test_pub_key_deserialization(bits in any::<[u8; 32]>()){
        let pt_deser = CompressedEdwardsY(bits).decompress();
        let pub_key = Ed25519PublicKey::try_from(&bits[..]);
        let check = matches!((pt_deser, pub_key),
            (Some(_), Ok(_)) // we agree with Dalek,
            | (Some(_), Err(CryptoMaterialError::SmallSubgroupError)) // dalek does not detect pubkeys in a small subgroup,
            | (None, Err(CryptoMaterialError::DeserializationError)) // we agree on point decompression failures,
        );
        prop_assert!(check);
    }

    #[test]
    fn test_keys_encode(keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>()) {
        {
            let encoded = keypair.private_key.to_encoded_string().unwrap();
            // Hex encoding of a 32-bytes key is 64 (2 x 32) characters.
            prop_assert_eq!(2 + 2 * ED25519_PRIVATE_KEY_LENGTH, encoded.len());
            let decoded = Ed25519PrivateKey::from_encoded_string(&encoded);
            prop_assert_eq!(Some(keypair.private_key), decoded.ok());
        }
        {
            let encoded = keypair.public_key.to_encoded_string().unwrap();
            // Hex encoding of a 32-bytes key is 64 (2 x 32) characters.
            prop_assert_eq!(2 + 2 * ED25519_PUBLIC_KEY_LENGTH, encoded.len());
            let decoded = Ed25519PublicKey::from_encoded_string(&encoded);
            prop_assert_eq!(Some(keypair.public_key), decoded.ok());
        }
    }

    #[test]
    fn test_batch_verify(
        message in random_serializable_struct(),
        keypairs in proptest::array::uniform10(uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>())
    ) {
        let mut signatures: Vec<(Ed25519PublicKey, Ed25519Signature)> = keypairs.iter().map(|keypair| {
            (keypair.public_key.clone(), keypair.private_key.sign(&message).unwrap())
        }).collect();
        prop_assert!(Ed25519Signature::batch_verify(&message, signatures.clone()).is_ok());
        // We swap message and signature for the last element,
        // resulting in an incorrect signature
        let (key, _sig) = signatures.pop().unwrap();
        let other_sig = signatures.last().unwrap().clone().1;
        signatures.push((key, other_sig));
        prop_assert!(Ed25519Signature::batch_verify(&message, signatures).is_err());
    }

    #[test]
    fn test_keys_custom_serialisation(
        keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>()
    ) {
        {
            let serialized: &[u8] = &(keypair.private_key.to_bytes());
            prop_assert_eq!(ED25519_PRIVATE_KEY_LENGTH, serialized.len());
            let deserialized = Ed25519PrivateKey::try_from(serialized);
            prop_assert_eq!(Some(keypair.private_key), deserialized.ok());
        }
        {
            let serialized: &[u8] = &(keypair.public_key.to_bytes());
            prop_assert_eq!(ED25519_PUBLIC_KEY_LENGTH, serialized.len());
            let deserialized = Ed25519PublicKey::try_from(serialized);
            prop_assert_eq!(Some(keypair.public_key), deserialized.ok());
        }
    }

    #[test]
    fn test_signature_verification_custom_serialisation(
        message in random_serializable_struct(),
        keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>()
    ) {
        let signature = keypair.private_key.sign(&message).unwrap();
        let serialized: &[u8] = &(signature.to_bytes());
        prop_assert_eq!(ED25519_SIGNATURE_LENGTH, serialized.len());
        let deserialized = Ed25519Signature::try_from(serialized).unwrap();
        prop_assert!(deserialized.verify(&message, &keypair.public_key).is_ok());
    }

    #[test]
    fn test_signature_verification_from_arbitrary(
        // this should be > 64 bits to go over the length of a default hash
        msg in vec(proptest::num::u8::ANY, 1..128),
        keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>()
    ) {
        let signature = keypair.private_key.sign_arbitrary_message(&msg);
        let serialized: &[u8] = &(signature.to_bytes());
        prop_assert_eq!(ED25519_SIGNATURE_LENGTH, serialized.len());
        let deserialized = Ed25519Signature::try_from(serialized).unwrap();
        prop_assert!(deserialized.verify_arbitrary_msg(&msg, &keypair.public_key).is_ok());
    }

    #[test]
    fn test_signature_verification_from_struct(
        x in any::<usize>(),
        keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>()
    ) {
        let hashable = CryptoHashable(x);
        let signature = keypair.private_key.sign(&hashable).unwrap();
        let serialized: &[u8] = &(signature.to_bytes());
        prop_assert_eq!(ED25519_SIGNATURE_LENGTH, serialized.len());
        let deserialized = Ed25519Signature::try_from(serialized).unwrap();
        prop_assert!(deserialized.verify(&hashable, &keypair.public_key).is_ok());
    }


    // Check for canonical S.
    #[test]
    fn test_signature_malleability(
        message in random_serializable_struct(),
        keypair in uniform_keypair_strategy::<Ed25519PrivateKey, Ed25519PublicKey>()
    ) {
        let signature = keypair.private_key.sign(&message).unwrap();
        let mut serialized = signature.to_bytes();
        let serialized_old = serialized; // implements Copy trait
        prop_assert_eq!(serialized_old, serialized);

        let mut r_bytes: [u8; 32] = [0u8; 32];
        r_bytes.copy_from_slice(&serialized[..32]);

        let mut s_bytes: [u8; 32] = [0u8; 32];
        s_bytes.copy_from_slice(&serialized[32..]);

        // ed25519-dalek signing ensures a canonical S value.
        let s = Scalar52::from_bytes(&s_bytes);

        // adding L (order of the base point) so that S + L > L
        let malleable_s = Scalar52::add(&s, &L);
        let malleable_s_bytes = malleable_s.to_bytes();
        // Update the signature (the S part).
        serialized[32..].copy_from_slice(&malleable_s_bytes);

        prop_assert_ne!(serialized_old, serialized);

        // Check that malleable signatures will pass verification and deserialization in dalek.
        // Construct the corresponding dalek public key.
        let _dalek_public_key = ed25519_dalek::PublicKey::from_bytes(
            &keypair.public_key.to_bytes()
        ).unwrap();

        // Construct the corresponding dalek Signature. This signature is malleable.
        let dalek_sig = ed25519_dalek::Signature::from_bytes(&serialized);

        // ed25519_dalek will (post 2.0) deserialize the malleable
        // signature. It does not detect it.
        prop_assert!(dalek_sig.is_ok());

        let msg_bytes = aptos_bcs::to_bytes(&message);
        prop_assert!(msg_bytes.is_ok());

        // ed25519_dalek verify will NOT accept the mauled signature
        prop_assert!(_dalek_public_key.verify(msg_bytes.as_ref().unwrap(), dalek_sig.as_ref().unwrap()).is_err());
        // ...and ed25519_dalek verify_strict will NOT accept it either
        prop_assert!(_dalek_public_key.verify_strict(msg_bytes.as_ref().unwrap(), dalek_sig.as_ref().unwrap()).is_err());
        // ...therefore, neither will our own Ed25519Signature::verify_arbitrary_msg
        let sig = Ed25519Signature::from_bytes_unchecked(&serialized).unwrap();
        prop_assert!(sig.verify(&message, &keypair.public_key).is_err());

        let serialized_malleable: &[u8] = &serialized;
        // try_from will fail on malleable signatures. We detect malleable signatures
        // early during deserialization.
        prop_assert_eq!(
            Ed25519Signature::try_from(serialized_malleable),
            Err(CryptoMaterialError::CanonicalRepresentationError)
        );

        // We expect from_bytes_unchecked deserialization to succeed, as dalek
        // does not check for signature malleability. This method is pub(crate)
        // and only used for test purposes.
        let sig_unchecked = Ed25519Signature::from_bytes_unchecked(&serialized);
        prop_assert!(sig_unchecked.is_ok());

        // Update the signature by setting S = L to make it invalid.
        serialized[32..].copy_from_slice(&L.to_bytes());
        let serialized_malleable_l: &[u8] = &serialized;
        // try_from will fail with CanonicalRepresentationError.
        prop_assert_eq!(
            Ed25519Signature::try_from(serialized_malleable_l),
            Err(CryptoMaterialError::CanonicalRepresentationError)
        );
    }

    // Test against known small subgroup public keys.
    #[allow(non_snake_case)]
    #[test]
    fn test_publickey_smallorder((R, A, m) in small_order_pk_with_adversarial_message()) {
        let pk_bytes = A.compress().to_bytes();

        // We expect from_bytes to pass in ed25519_dalek, as it does not validate the PK.
        let pk_dalek = ed25519_dalek::PublicKey::from_bytes(&pk_bytes);
        prop_assert!(pk_dalek.is_ok());
        let pk_dalek = pk_dalek.unwrap();

        // We expect from_bytes_unchecked to pass, as it does not validate the PK.
        let pk = Ed25519PublicKey::from_bytes_unchecked(&pk_bytes);
        prop_assert!(pk.is_ok());
        let pk = pk.unwrap();

        // Ensure the order of the PK is small
        prop_assert!(EIGHT_TORSION.len() <= 8);
        prop_assert!(eight_torsion_order(A) <= EIGHT_TORSION.len());

        // Verification checks sB - hA = R. We set s = 0, and we get R + hA = Identity. We set R to
        // be a small order element, and all we have to do is find a message with any hash h such
        // that R + hA = Identity.
        let s = Scalar::zero();

        let sig_bytes : Vec<u8> = [R.compress().to_bytes(), s.to_bytes()].concat();
        let sig_dalek = ed25519_dalek::Signature::from_bytes(&sig_bytes).unwrap();

        // We expect ed25519-dalek verify to succeed
        prop_assert!(pk_dalek.verify(signing_message(&m).unwrap().as_ref(), &sig_dalek).is_ok());

        // We expect ed25519-dalek verify_strict to fail
        prop_assert!(pk_dalek.verify_strict(signing_message(&m).unwrap().as_ref(), &sig_dalek).is_err());

        // We expect our own validation to fail in Ed25519Signature::verify_arbitrary_msg, since it
        // calls ed25519-dalek's verify_strict
        let sig = Ed25519Signature::from_bytes_unchecked(sig_bytes.as_ref()).unwrap();
        prop_assert!(pk.verify_struct_signature(&m, &sig).is_err());
    }
}

// The 8-torsion subgroup E[8].
//
// In the case of Curve25519, it is cyclic; the i-th element of
// the array is [i]P, where P is a point of order 8
// generating E[8].
//
// Thus E[8] is the points indexed by `0,2,4,6`, and
// E[2] is the points indexed by `0,4`.
//
// The following byte arrays have been ported from curve25519-dalek /backend/serial/u64/constants.rs
// and they represent the serialised version of the CompressedEdwardsY points.

pub const EIGHT_TORSION: [[u8; 32]; 8] = [
    [
        1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ],
    [
        199, 23, 106, 112, 61, 77, 216, 79, 186, 60, 11, 118, 13, 16, 103, 15, 42, 32, 83, 250, 44,
        57, 204, 198, 78, 199, 253, 119, 146, 172, 3, 122,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 128,
    ],
    [
        38, 232, 149, 143, 194, 178, 39, 176, 69, 195, 244, 137, 242, 239, 152, 240, 213, 223, 172,
        5, 211, 198, 51, 57, 177, 56, 2, 136, 109, 83, 252, 5,
    ],
    [
        236, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 127,
    ],
    [
        38, 232, 149, 143, 194, 178, 39, 176, 69, 195, 244, 137, 242, 239, 152, 240, 213, 223, 172,
        5, 211, 198, 51, 57, 177, 56, 2, 136, 109, 83, 252, 133,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ],
    [
        199, 23, 106, 112, 61, 77, 216, 79, 186, 60, 11, 118, 13, 16, 103, 15, 42, 32, 83, 250, 44,
        57, 204, 198, 78, 199, 253, 119, 146, 172, 3, 250,
    ],
];

/// The `Scalar52` struct represents an element in
/// ℤ/ℓℤ as 5 52-bit limbs.
pub struct Scalar52(pub [u64; 5]);

/// `L` is the order of base point, i.e. 2^252 + 27742317777372353535851937790883648493
pub const L: Scalar52 = Scalar52([
    0x0002_631A_5CF5_D3ED,
    0x000D_EA2F_79CD_6581,
    0x0000_0000_0014_DEF9,
    0x0000_0000_0000_0000,
    0x0000_1000_0000_0000,
]);

impl Scalar52 {
    /// Return the zero scalar
    fn zero() -> Scalar52 {
        Scalar52([0, 0, 0, 0, 0])
    }

    /// Unpack a 32 byte / 256 bit scalar into 5 52-bit limbs.
    pub fn from_bytes(bytes: &[u8; 32]) -> Scalar52 {
        let mut words = [0u64; 4];
        for i in 0..4 {
            for j in 0..8 {
                words[i] |= u64::from(bytes[(i * 8) + j]) << (j * 8) as u64;
            }
        }

        let mask = (1u64 << 52) - 1;
        let top_mask = (1u64 << 48) - 1;
        let mut s = Scalar52::zero();

        s[0] = words[0] & mask;
        s[1] = ((words[0] >> 52) | (words[1] << 12)) & mask;
        s[2] = ((words[1] >> 40) | (words[2] << 24)) & mask;
        s[3] = ((words[2] >> 28) | (words[3] << 36)) & mask;
        s[4] = (words[3] >> 16) & top_mask;

        s
    }

    /// Pack the limbs of this `Scalar52` into 32 bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut s = [0u8; 32];

        s[0] = self.0[0] as u8;
        s[1] = (self.0[0] >> 8) as u8;
        s[2] = (self.0[0] >> 16) as u8;
        s[3] = (self.0[0] >> 24) as u8;
        s[4] = (self.0[0] >> 32) as u8;
        s[5] = (self.0[0] >> 40) as u8;
        s[6] = ((self.0[0] >> 48) | (self.0[1] << 4)) as u8;
        s[7] = (self.0[1] >> 4) as u8;
        s[8] = (self.0[1] >> 12) as u8;
        s[9] = (self.0[1] >> 20) as u8;
        s[10] = (self.0[1] >> 28) as u8;
        s[11] = (self.0[1] >> 36) as u8;
        s[12] = (self.0[1] >> 44) as u8;
        s[13] = self.0[2] as u8;
        s[14] = (self.0[2] >> 8) as u8;
        s[15] = (self.0[2] >> 16) as u8;
        s[16] = (self.0[2] >> 24) as u8;
        s[17] = (self.0[2] >> 32) as u8;
        s[18] = (self.0[2] >> 40) as u8;
        s[19] = ((self.0[2] >> 48) | (self.0[3] << 4)) as u8;
        s[20] = (self.0[3] >> 4) as u8;
        s[21] = (self.0[3] >> 12) as u8;
        s[22] = (self.0[3] >> 20) as u8;
        s[23] = (self.0[3] >> 28) as u8;
        s[24] = (self.0[3] >> 36) as u8;
        s[25] = (self.0[3] >> 44) as u8;
        s[26] = self.0[4] as u8;
        s[27] = (self.0[4] >> 8) as u8;
        s[28] = (self.0[4] >> 16) as u8;
        s[29] = (self.0[4] >> 24) as u8;
        s[30] = (self.0[4] >> 32) as u8;
        s[31] = (self.0[4] >> 40) as u8;

        s
    }

    /// Compute `a + b` (without mod ℓ)
    pub fn add(a: &Scalar52, b: &Scalar52) -> Scalar52 {
        let mut sum = Scalar52::zero();
        let mask = (1u64 << 52) - 1;

        // a + b
        let mut carry: u64 = 0;
        for i in 0..5 {
            carry = a[i] + b[i] + (carry >> 52);
            sum[i] = carry & mask;
        }

        sum
    }
}

impl Index<usize> for Scalar52 {
    type Output = u64;

    fn index(&self, _index: usize) -> &u64 {
        &(self.0[_index])
    }
}

impl IndexMut<usize> for Scalar52 {
    fn index_mut(&mut self, _index: usize) -> &mut u64 {
        &mut (self.0[_index])
    }
}
