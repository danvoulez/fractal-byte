//! JWS detached signing for transition receipts.
//!
//! Produces a JWS compact serialization with `b64=false` (detached payload).
//! The payload (canonical body bytes) is NOT embedded in the JWS — it travels
//! alongside, so the CID of the body is never altered by the signature.
//!
//! Format: `<header_b64url>...<signature_b64url>` (empty payload segment)

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

const B64_URL: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JwsDetached {
    pub protected: String,
    pub signature: String,
    pub kid: String,
}

/// Sign `payload` (canonical body bytes) with Ed25519, producing a JWS detached envelope.
///
/// The signing input is `<protected_b64url>.<payload_bytes>` per RFC 7797 (b64=false).
pub fn sign_detached(payload: &[u8], key: &SigningKey, kid: &str) -> JwsDetached {
    let header = serde_json::json!({
        "alg": "EdDSA",
        "b64": false,
        "crit": ["b64"],
        "kid": kid,
        "typ": "ubl/rc+json"
    });
    let protected = B64_URL.encode(serde_json::to_vec(&header).unwrap());

    // RFC 7797 §5.1: signing input = ASCII(BASE64URL(header)) || '.' || payload_bytes
    let mut signing_input = Vec::with_capacity(protected.len() + 1 + payload.len());
    signing_input.extend_from_slice(protected.as_bytes());
    signing_input.push(b'.');
    signing_input.extend_from_slice(payload);

    let sig = key.sign(&signing_input);
    let signature = B64_URL.encode(sig.to_bytes());

    JwsDetached {
        protected,
        signature,
        kid: kid.to_string(),
    }
}

/// Verify a JWS detached signature against the original payload bytes.
pub fn verify_detached(
    jws: &JwsDetached,
    payload: &[u8],
    verifying_key: &ed25519_dalek::VerifyingKey,
) -> bool {
    use ed25519_dalek::Verifier;

    let sig_bytes = match B64_URL.decode(&jws.signature) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig = match ed25519_dalek::Signature::from_slice(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut signing_input = Vec::with_capacity(jws.protected.len() + 1 + payload.len());
    signing_input.extend_from_slice(jws.protected.as_bytes());
    signing_input.push(b'.');
    signing_input.extend_from_slice(payload);

    verifying_key.verify(&signing_input, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[test]
    fn sign_and_verify() {
        let key = test_key();
        let payload = b"canonical body bytes";
        let jws = sign_detached(payload, &key, "did:dev#k1");

        assert!(!jws.protected.is_empty());
        assert!(!jws.signature.is_empty());
        assert_eq!(jws.kid, "did:dev#k1");

        let vk = key.verifying_key();
        assert!(verify_detached(&jws, payload, &vk), "signature must verify");
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let key = test_key();
        let payload = b"original";
        let jws = sign_detached(payload, &key, "did:dev#k1");

        let vk = key.verifying_key();
        assert!(!verify_detached(&jws, b"tampered", &vk), "tampered payload must fail");
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let key = test_key();
        let payload = b"data";
        let jws = sign_detached(payload, &key, "did:dev#k1");

        let wrong_key = SigningKey::from_bytes(&[99u8; 32]);
        let wrong_vk = wrong_key.verifying_key();
        assert!(!verify_detached(&jws, payload, &wrong_vk), "wrong key must fail");
    }

    #[test]
    fn deterministic_signature() {
        let key = test_key();
        let payload = b"deterministic test";
        let jws1 = sign_detached(payload, &key, "did:dev#k1");
        let jws2 = sign_detached(payload, &key, "did:dev#k1");
        assert_eq!(jws1.signature, jws2.signature, "Ed25519 is deterministic");
        assert_eq!(jws1.protected, jws2.protected);
    }

    #[test]
    fn protected_header_contains_b64_false() {
        let key = test_key();
        let jws = sign_detached(b"test", &key, "did:dev#k1");
        let decoded = B64_URL.decode(&jws.protected).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(header["b64"], false);
        assert_eq!(header["alg"], "EdDSA");
        assert_eq!(header["typ"], "ubl/rc+json");
    }
}
