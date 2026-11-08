use crate::seal::parse_public;
use crate::{b64, b64_decode, Error, Result};
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;

pub fn public_key(secret: &[u8]) -> Result<Vec<u8>> {
    let key = p256::SecretKey::from_slice(secret).map_err(|_| Error::BadKey)?;
    Ok(key.public_key().to_encoded_point(false).as_bytes().to_vec())
}

pub fn sign(sign_secret: &[u8], message: &str) -> Result<String> {
    let key = SigningKey::from_slice(sign_secret).map_err(|_| Error::BadKey)?;
    let sig: Signature = key.sign(message.as_bytes());
    let sig = sig.normalize_s().unwrap_or(sig);
    Ok(b64(sig.to_der().as_bytes()))
}

pub fn verify(sign_pub: &[u8], message: &str, sig_b64: &str) -> Result<()> {
    let key = VerifyingKey::from(parse_public(sign_pub)?);
    let raw = b64_decode(sig_b64).map_err(|_| Error::BadSignature)?;
    let sig = if raw.len() == 64 {
        Signature::from_slice(&raw)
    } else {
        Signature::from_der(&raw)
    }
    .map_err(|_| Error::BadSignature)?;
    let sig = sig.normalize_s().unwrap_or(sig);
    key.verify(message.as_bytes(), &sig)
        .map_err(|_| Error::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitted_signatures_are_low_s() {
        let secret = [7u8; 32];
        for i in 0..64 {
            let sig = sign(&secret, &format!("message {i}")).unwrap();
            let parsed = Signature::from_der(&b64_decode(&sig).unwrap()).unwrap();
            assert!(parsed.normalize_s().is_none());
            verify(&public_key(&secret).unwrap(), &format!("message {i}"), &sig).unwrap();
        }
    }
}
