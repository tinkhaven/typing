//! Just enough JWT to verify an RS256 ID token.
//!
//! # Why not a JWT library
//!
//! This replaced `jsonwebtoken`, whose only RSA backend that builds without a C
//! toolchain pulls in the `rsa` crate — and `rsa` carries RUSTSEC-2023-0071, the
//! Marvin timing sidechannel, for which no fixed release exists. Excusing a live
//! advisory in an authentication path is not a good look even when the attack
//! does not apply, and the alternative backend needs cmake in every build.
//!
//! `ring` is already in the dependency graph (rustls uses it), has no advisory,
//! and does PKCS#1 v1.5 verification directly. What is left to write is the
//! base64 and JSON around it, which is this file: three segments, one signature
//! check, and no key parsing of our own.
//!
//! Deliberately verification-only. There is no signing here, no HMAC algorithms,
//! and `alg` is not read from the token to choose a verifier — the caller says
//! it expects RS256 and that is what is used, which is how the classic "alg:
//! none" and algorithm-confusion attacks are avoided.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::signature::{RsaPublicKeyComponents, RSA_PKCS1_2048_8192_SHA256};
use serde::Deserialize;

/// A provider's published signing keys.
#[derive(Clone, Debug, Deserialize)]
pub struct Jwks {
    /// The keys, in whatever order the provider listed them.
    pub keys: Vec<Jwk>,
}

/// One published key.
#[derive(Clone, Debug, Deserialize)]
pub struct Jwk {
    /// Key type; only `RSA` is usable here.
    pub kty: String,
    /// Key id, matched against the token's header.
    pub kid: Option<String>,
    /// RSA modulus, base64url.
    pub n: Option<String>,
    /// RSA exponent, base64url.
    pub e: Option<String>,
}

impl Jwks {
    /// The key with this id, if it is an RSA key with usable components.
    fn find_rsa(&self, kid: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        let jwk = self
            .keys
            .iter()
            .find(|key| key.kid.as_deref() == Some(kid) && key.kty == "RSA")?;
        let n = URL_SAFE_NO_PAD.decode(jwk.n.as_deref()?).ok()?;
        let e = URL_SAFE_NO_PAD.decode(jwk.e.as_deref()?).ok()?;
        Some((n, e))
    }
}

/// The header fields that matter.
#[derive(Clone, Debug, Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

/// Why a token was not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JwtError {
    /// Not three dot-separated base64url segments.
    Malformed,
    /// The header named an algorithm other than RS256.
    ///
    /// Refused rather than honoured: taking the algorithm from the token is how
    /// `alg: none` and HMAC-with-the-public-key attacks get in.
    UnexpectedAlgorithm(String),
    /// The header carried no key id, so the right key cannot be chosen.
    NoKeyId,
    /// The provider's key set has no such key.
    UnknownKeyId(String),
    /// The signature did not verify against that key.
    BadSignature,
    /// The payload was not the JSON the caller expected.
    BadClaims(String),
}

impl core::fmt::Display for JwtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            JwtError::Malformed => write!(f, "not a well-formed JWT"),
            JwtError::UnexpectedAlgorithm(alg) => {
                write!(f, "token is signed with {alg}, only RS256 is accepted")
            }
            JwtError::NoKeyId => write!(f, "token header carries no key id"),
            JwtError::UnknownKeyId(kid) => write!(f, "no published key with id {kid}"),
            JwtError::BadSignature => write!(f, "signature does not verify"),
            JwtError::BadClaims(why) => write!(f, "unreadable claims: {why}"),
        }
    }
}

impl std::error::Error for JwtError {}

/// The key id a token says it was signed with.
pub fn key_id(token: &str) -> Result<String, JwtError> {
    header(token)?.kid.ok_or(JwtError::NoKeyId)
}

fn header(token: &str) -> Result<Header, JwtError> {
    let segment = token.split('.').next().ok_or(JwtError::Malformed)?;
    let raw = URL_SAFE_NO_PAD
        .decode(segment)
        .map_err(|_| JwtError::Malformed)?;
    serde_json::from_slice(&raw).map_err(|_| JwtError::Malformed)
}

/// Verifies an RS256 signature against a key set and returns the claims.
///
/// Signature first, always: nothing in the payload is worth reading until it is
/// established that the provider produced it. Claim checks are the caller's,
/// since which issuer and audience are acceptable is not this module's business.
pub fn verify_rs256<T: serde::de::DeserializeOwned>(
    token: &str,
    keys: &Jwks,
) -> Result<T, JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    let [header_segment, payload_segment, signature_segment] = parts.as_slice() else {
        return Err(JwtError::Malformed);
    };

    let header = header(token)?;
    if header.alg != "RS256" {
        return Err(JwtError::UnexpectedAlgorithm(header.alg));
    }
    let kid = header.kid.ok_or(JwtError::NoKeyId)?;
    let (n, e) = keys.find_rsa(&kid).ok_or(JwtError::UnknownKeyId(kid))?;

    let signature = URL_SAFE_NO_PAD
        .decode(signature_segment)
        .map_err(|_| JwtError::Malformed)?;
    // What was signed is the first two segments and the dot between them,
    // exactly as they arrived — re-encoding them could change a byte.
    let signed = format!("{header_segment}.{payload_segment}");

    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(&RSA_PKCS1_2048_8192_SHA256, signed.as_bytes(), &signature)
        .map_err(|_| JwtError::BadSignature)?;

    let payload = URL_SAFE_NO_PAD
        .decode(payload_segment)
        .map_err(|_| JwtError::Malformed)?;
    serde_json::from_slice(&payload).map_err(|e| JwtError::BadClaims(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A genuinely RS256-signed token and the public key that verifies it.
    ///
    /// Generated once with OpenSSL; the private key was discarded, so only the
    /// token and the public modulus are here. Its expiry is in 2100 so the
    /// vector does not rot.
    const VALID: &str = include_str!("../../tests/vectors/valid.jwt");
    const TAMPERED_PAYLOAD: &str = include_str!("../../tests/vectors/wrong_issuer.jwt");
    const UNKNOWN_KID: &str = include_str!("../../tests/vectors/unknown_kid.jwt");
    const MODULUS: &str = include_str!("../../tests/vectors/modulus.b64");
    const EXPONENT: &str = "AQAB";

    #[derive(Debug, PartialEq, Deserialize)]
    struct Claims {
        iss: String,
        sub: String,
        aud: String,
        exp: u64,
    }

    fn keys() -> Jwks {
        Jwks {
            keys: vec![Jwk {
                kty: "RSA".into(),
                kid: Some("vector-key-1".into()),
                n: Some(MODULUS.trim().into()),
                e: Some(EXPONENT.into()),
            }],
        }
    }

    #[test]
    fn a_genuine_token_verifies_and_decodes() {
        let claims: Claims = verify_rs256(VALID.trim(), &keys()).expect("verifies");
        assert_eq!(claims.iss, "https://accounts.google.test");
        assert_eq!(claims.sub, "subject-108121");
        assert_eq!(claims.aud, "test-client.apps.googleusercontent.com");
        assert!(claims.exp > 4_000_000_000, "the vector should not expire");
    }

    #[test]
    fn a_flipped_signature_byte_is_caught() {
        let token = VALID.trim();
        let (body, signature) = token.rsplit_once('.').expect("three segments");
        // Change one character of the signature.
        let mut bytes: Vec<char> = signature.chars().collect();
        bytes[0] = if bytes[0] == 'A' { 'B' } else { 'A' };
        let forged = format!("{body}.{}", bytes.into_iter().collect::<String>());
        assert_eq!(
            verify_rs256::<Claims>(&forged, &keys()),
            Err(JwtError::BadSignature)
        );
    }

    #[test]
    fn swapping_the_payload_for_another_signed_one_is_caught() {
        // Take the valid token's signature and a different token's payload: the
        // signature covers header.payload, so this must not verify.
        let valid = VALID.trim();
        let other = TAMPERED_PAYLOAD.trim();
        let signature = valid.rsplit_once('.').expect("segments").1;
        let (other_body, _) = other.rsplit_once('.').expect("segments");
        let forged = format!("{other_body}.{signature}");
        assert_eq!(
            verify_rs256::<Claims>(&forged, &keys()),
            Err(JwtError::BadSignature)
        );
    }

    #[test]
    fn a_token_signed_by_an_unpublished_key_is_refused() {
        let verdict = verify_rs256::<Claims>(UNKNOWN_KID.trim(), &keys());
        assert!(
            matches!(verdict, Err(JwtError::UnknownKeyId(_))),
            "{verdict:?}"
        );
    }

    #[test]
    fn the_algorithm_is_not_taken_from_the_token() {
        // "alg": "none" with an empty signature — the classic attack. The header
        // is rewritten but the rest of the token left alone.
        let token = VALID.trim();
        let payload = token.split('.').nth(1).expect("payload");
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT","kid":"vector-key-1"}"#);
        let forged = format!("{header}.{payload}.");
        assert_eq!(
            verify_rs256::<Claims>(&forged, &keys()),
            Err(JwtError::UnexpectedAlgorithm("none".into()))
        );
    }

    #[test]
    fn an_hmac_algorithm_is_refused_rather_than_honoured() {
        // Algorithm confusion: signing with HS256 using the RSA public key as
        // the HMAC secret. Refusing anything but RS256 closes it outright.
        let token = VALID.trim();
        let payload = token.split('.').nth(1).expect("payload");
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT","kid":"vector-key-1"}"#);
        let forged = format!("{header}.{payload}.c2lnbmF0dXJl");
        assert_eq!(
            verify_rs256::<Claims>(&forged, &keys()),
            Err(JwtError::UnexpectedAlgorithm("HS256".into()))
        );
    }

    #[test]
    fn malformed_tokens_are_refused_without_panicking() {
        for raw in ["", "a", "a.b", "a.b.c.d", "....", "!!!.???.***"] {
            assert!(verify_rs256::<Claims>(raw, &keys()).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn a_key_set_without_the_right_key_type_is_no_help() {
        let elliptic = Jwks {
            keys: vec![Jwk {
                kty: "EC".into(),
                kid: Some("vector-key-1".into()),
                n: None,
                e: None,
            }],
        };
        assert!(matches!(
            verify_rs256::<Claims>(VALID.trim(), &elliptic),
            Err(JwtError::UnknownKeyId(_))
        ));
    }

    #[test]
    fn the_key_id_can_be_read_before_verifying() {
        assert_eq!(key_id(VALID.trim()).unwrap(), "vector-key-1");
        assert_eq!(key_id("not-a-token"), Err(JwtError::Malformed));
    }
}
