use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::SecretKey;
use tmgr_phone::{b64, h, Error, Result};

pub struct DeviceKeys {
    sign_secret: [u8; 32],
    kex_secret: [u8; 32],
}

fn random_secret() -> Result<[u8; 32]> {
    loop {
        let mut s = [0u8; 32];
        getrandom::fill(&mut s).map_err(|_| Error::Random)?;
        if SecretKey::from_slice(&s).is_ok() {
            return Ok(s);
        }
    }
}

fn public_b64(secret: &[u8]) -> String {
    let key = SecretKey::from_slice(secret).expect("validated at construction");
    b64(key.public_key().to_encoded_point(false).as_bytes())
}

pub fn fingerprint(sign_pub_b64: &str, kex_pub_b64: &str) -> String {
    h(&format!("tmgr-chat-fp:v1:{sign_pub_b64}:{kex_pub_b64}"))
}

impl DeviceKeys {
    pub fn generate() -> Result<DeviceKeys> {
        Ok(DeviceKeys {
            sign_secret: random_secret()?,
            kex_secret: random_secret()?,
        })
    }

    pub fn from_secrets(sign_secret: &[u8], kex_secret: &[u8]) -> Result<DeviceKeys> {
        let sign_secret: [u8; 32] = sign_secret.try_into().map_err(|_| Error::BadKey)?;
        let kex_secret: [u8; 32] = kex_secret.try_into().map_err(|_| Error::BadKey)?;
        SecretKey::from_slice(&sign_secret).map_err(|_| Error::BadKey)?;
        SecretKey::from_slice(&kex_secret).map_err(|_| Error::BadKey)?;
        Ok(DeviceKeys {
            sign_secret,
            kex_secret,
        })
    }

    pub fn sign_secret(&self) -> &[u8; 32] {
        &self.sign_secret
    }

    pub fn kex_secret(&self) -> &[u8; 32] {
        &self.kex_secret
    }

    pub fn sign_pub_b64(&self) -> String {
        public_b64(&self.sign_secret)
    }

    pub fn kex_pub_b64(&self) -> String {
        public_b64(&self.kex_secret)
    }

    pub fn fingerprint(&self) -> String {
        fingerprint(&self.sign_pub_b64(), &self.kex_pub_b64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_phone::b64_decode;

    fn fixed() -> DeviceKeys {
        DeviceKeys::from_secrets(&[1u8; 32], &[2u8; 32]).unwrap()
    }

    #[test]
    fn public_keys_are_65_byte_uncompressed_padded_b64() {
        let k = fixed();
        for p in [k.sign_pub_b64(), k.kex_pub_b64()] {
            let raw = b64_decode(&p).unwrap();
            assert_eq!(raw.len(), 65);
            assert_eq!(raw[0], 4);
            assert!(p.ends_with('='));
        }
        assert_ne!(k.sign_pub_b64(), k.kex_pub_b64());
    }

    #[test]
    fn fingerprint_is_hex_sha256_of_spec_string() {
        let k = fixed();
        let expect = h(&format!(
            "tmgr-chat-fp:v1:{}:{}",
            k.sign_pub_b64(),
            k.kex_pub_b64()
        ));
        assert_eq!(k.fingerprint(), expect);
        assert_eq!(k.fingerprint().len(), 64);
        assert!(k
            .fingerprint()
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn generate_gives_distinct_valid_keys() {
        let a = DeviceKeys::generate().unwrap();
        let b = DeviceKeys::generate().unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
        DeviceKeys::from_secrets(a.sign_secret(), a.kex_secret()).unwrap();
    }

    #[test]
    fn bad_secrets_are_refused() {
        assert!(DeviceKeys::from_secrets(&[0u8; 32], &[2u8; 32]).is_err());
        assert!(DeviceKeys::from_secrets(&[1u8; 31], &[2u8; 32]).is_err());
    }
}
