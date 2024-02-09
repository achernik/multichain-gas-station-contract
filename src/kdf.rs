// From: https://github.com/near/mpc-recovery/blob/bc85d66833ffa8537ec61d0b22cd5aa96fbe3197/node/src/kdf.rs

use ethers_core::k256::{
    elliptic_curve::{
        scalar::FromUintUnchecked,
        sec1::{FromEncodedPoint, Tag, ToEncodedPoint},
        CurveArithmetic,
    },
    AffinePoint, EncodedPoint, Scalar, Secp256k1, U256,
};
use near_sdk::AccountId;

use crate::foreign_address::ForeignAddress;

pub type PublicKey = <Secp256k1 as CurveArithmetic>::AffinePoint;

pub trait ScalarExt {
    fn from_bytes(bytes: &[u8]) -> Self;
}

impl ScalarExt for Scalar {
    fn from_bytes(bytes: &[u8]) -> Self {
        Scalar::from_uint_unchecked(U256::from_le_slice(bytes))
    }
}

#[cfg(target_arch = "wasm32")]
fn sha256(bytes: &[u8]) -> Vec<u8> {
    near_sdk::env::sha256(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
fn sha256(bytes: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hasher.finalize().to_vec()
}

// Constant prefix that ensures epsilon derivation values are used specifically for
// near-mpc-recovery with key derivation protocol vX.Y.Z.
const EPSILON_DERIVATION_PREFIX: &str = "near-mpc-recovery v0.1.0 epsilon derivation:";

#[must_use]
pub fn derive_epsilon(signer_id: &AccountId, path: &str) -> Scalar {
    let derivation_path = format!("{EPSILON_DERIVATION_PREFIX}{signer_id},{path}");
    Scalar::from_bytes(&sha256(derivation_path.as_bytes()))
}

#[must_use]
pub fn derive_key(public_key: PublicKey, epsilon: Scalar) -> PublicKey {
    (<Secp256k1 as CurveArithmetic>::ProjectivePoint::GENERATOR * epsilon + public_key).to_affine()
}

#[must_use]
pub fn derive_key_for_account(
    mpc_public_key: PublicKey,
    account_id: &AccountId,
    path: &str,
) -> ethers_core::types::Address {
    let epsilon = derive_epsilon(account_id, path);
    let affine_point = derive_key(mpc_public_key, epsilon);
    let encoded = affine_point.to_encoded_point(false);
    ethers_core::utils::raw_public_key_to_address(&encoded.as_bytes()[1..])
}

#[derive(Debug, thiserror::Error)]
pub enum PublicKeyConversionError {
    #[error("Can only convert from SECP256K1")]
    WrongCurveType(near_sdk::CurveType),
    #[error("Decoding error")]
    DecodingError(#[from] ethers_core::k256::elliptic_curve::Error),
    #[error("Invalid key data")]
    InvalidKeyData,
}

/// Converts an SECP256K1-variant [`near_sdk::PublicKey`] to an [`AffinePoint`].
///
/// # Errors
///
/// Returns an error if the public key is not a valid SECP256K1 key.
pub fn near_public_key_to_affine(
    public_key: near_sdk::PublicKey,
) -> Result<AffinePoint, PublicKeyConversionError> {
    // wasm only
    #[cfg(target_arch = "wasm32")]
    {
        let curve_type = public_key.curve_type();
        if curve_type != near_sdk::CurveType::SECP256K1 {
            return Err(PublicKeyConversionError::WrongCurveType(curve_type));
        }
    }

    let mut bytes = public_key.into_bytes();
    bytes[0] = u8::from(Tag::Uncompressed);

    let affine: Option<AffinePoint> = AffinePoint::from_encoded_point(
        &EncodedPoint::from_bytes(&bytes)
            .map_err(|e| PublicKeyConversionError::DecodingError(e.into()))?,
    )
    .into();

    affine.ok_or(PublicKeyConversionError::InvalidKeyData)
}

/// TODO: The value returned by this function conflicts with that recovered from signatures. Requires further investigation.
/// Calculates the public key of the MPC signer for the given account ID and derivation path.
///
/// # Errors
///
/// Returns an error if the public key is not a valid SECP256K1 key.
pub fn get_mpc_address(
    mpc_public_key: near_sdk::PublicKey,
    gas_station_account_id: &AccountId,
    caller_account_id: &str,
) -> Result<ForeignAddress, PublicKeyConversionError> {
    let affine = near_public_key_to_affine(mpc_public_key)?;
    Ok(derive_key_for_account(affine, gas_station_account_id, caller_account_id).into())
}

#[test]
fn test_keys() {
    let public_key: near_sdk::PublicKey = "secp256k1:47xve2ymatpG4x4Gp7pmYwuLJk7eeRegrFuS4VoW5VV4i3GsBiBY87vkH6UZiiY18NeZnkBzcZzipDbJJ5pmjTcc"
        .parse()
        .unwrap();

    let a = near_public_key_to_affine(public_key.clone()).unwrap();

    let mpc_address =
        derive_key_for_account(a, &"canhazgas.testnet".parse().unwrap(), "hatchet.testnet");

    println!("{}", ethers_core::utils::to_checksum(&mpc_address, None));
}
