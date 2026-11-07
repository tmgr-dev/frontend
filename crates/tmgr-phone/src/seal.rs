use crate::{b64, b64_decode, Error, Result};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    D2p,
    P2d,
}

impl Dir {
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::D2p => "d2p",
            Dir::P2d => "p2d",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Hello,
    PairOk,
    Snapshot,
    Answer,
    Ack,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Hello => "hello",
            Kind::PairOk => "pair_ok",
            Kind::Snapshot => "snapshot",
            Kind::Answer => "answer",
            Kind::Ack => "ack",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        [
            Kind::Hello,
            Kind::PairOk,
            Kind::Snapshot,
            Kind::Answer,
            Kind::Ack,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

#[derive(Clone, Debug)]
pub struct Entropy {
    pub eph_secret: [u8; 32],
    pub nonce: [u8; 12],
}

impl Entropy {
    pub fn random() -> Result<Entropy> {
        loop {
            let mut eph_secret = [0u8; 32];
            let mut nonce = [0u8; 12];
            getrandom::fill(&mut eph_secret).map_err(|_| Error::Random)?;
            getrandom::fill(&mut nonce).map_err(|_| Error::Random)?;
            if SecretKey::from_slice(&eph_secret).is_ok() {
                return Ok(Entropy { eph_secret, nonce });
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Sealed {
    pub v: u32,
    pub kind: String,
    pub id: String,
    pub eph: String,
    pub nonce: String,
    pub ct: String,
}

pub struct Opened {
    pub kind: Kind,
    pub id: String,
    pub plaintext: Vec<u8>,
}

pub fn ctx(mailbox: &str, dir: Dir, kind: &str, id: &str) -> String {
    format!("tmgr-phone:v1:{mailbox}:{}:{kind}:{id}", dir.as_str())
}

pub fn parse_public(bytes: &[u8]) -> Result<PublicKey> {
    if bytes.len() != 65 || bytes[0] != 4 {
        return Err(Error::BadKey);
    }
    PublicKey::from_sec1_bytes(bytes).map_err(|_| Error::BadKey)
}

fn derive(shared: &[u8], eph_pub: &[u8], recipient_pub: &[u8], ctx: &str) -> Result<Aes256Gcm> {
    let salt = [eph_pub, recipient_pub].concat();
    let info = format!("tmgr-phone-seal:v1:{ctx}");
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared)
        .expand(info.as_bytes(), &mut key)
        .map_err(|_| Error::OpenFailed)?;
    Aes256Gcm::new_from_slice(&key).map_err(|_| Error::OpenFailed)
}

pub fn seal_with(
    recipient_kex_pub: &[u8],
    mailbox: &str,
    dir: Dir,
    kind: Kind,
    id: &str,
    plaintext: &[u8],
    ent: &Entropy,
) -> Result<Sealed> {
    let recipient = parse_public(recipient_kex_pub)?;
    let eph = SecretKey::from_slice(&ent.eph_secret).map_err(|_| Error::BadKey)?;
    let eph_pub = eph.public_key().to_encoded_point(false);
    let shared = diffie_hellman(eph.to_nonzero_scalar(), recipient.as_affine());
    let ctx = ctx(mailbox, dir, kind.as_str(), id);
    let cipher = derive(
        shared.raw_secret_bytes().as_slice(),
        eph_pub.as_bytes(),
        recipient_kex_pub,
        &ctx,
    )?;
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&ent.nonce),
            Payload {
                msg: plaintext,
                aad: ctx.as_bytes(),
            },
        )
        .map_err(|_| Error::OpenFailed)?;
    Ok(Sealed {
        v: 1,
        kind: kind.as_str().to_string(),
        id: id.to_string(),
        eph: b64(eph_pub.as_bytes()),
        nonce: b64(&ent.nonce),
        ct: b64(&ct),
    })
}

pub fn seal(
    recipient_kex_pub: &[u8],
    mailbox: &str,
    dir: Dir,
    kind: Kind,
    id: &str,
    plaintext: &[u8],
) -> Result<Sealed> {
    seal_with(
        recipient_kex_pub,
        mailbox,
        dir,
        kind,
        id,
        plaintext,
        &Entropy::random()?,
    )
}

pub fn open(
    kex_secret: &[u8],
    mailbox: &str,
    dir: Dir,
    sealed_json: &str,
    expected: &[Kind],
) -> Result<Opened> {
    let sealed: Sealed = serde_json::from_str(sealed_json).map_err(|_| Error::BadJson)?;
    if sealed.v != 1 {
        return Err(Error::BadFormat);
    }
    let kind = Kind::parse(&sealed.kind)
        .filter(|k| expected.contains(k))
        .ok_or(Error::UnexpectedKind)?;
    let secret = SecretKey::from_slice(kex_secret).map_err(|_| Error::BadKey)?;
    let own_pub = secret.public_key().to_encoded_point(false);
    let eph_bytes = b64_decode(&sealed.eph)?;
    let eph = parse_public(&eph_bytes)?;
    let nonce = b64_decode(&sealed.nonce)?;
    if nonce.len() != 12 {
        return Err(Error::BadFormat);
    }
    let ct = b64_decode(&sealed.ct)?;
    let shared = diffie_hellman(secret.to_nonzero_scalar(), eph.as_affine());
    let ctx = ctx(mailbox, dir, kind.as_str(), &sealed.id);
    let cipher = derive(
        shared.raw_secret_bytes().as_slice(),
        &eph_bytes,
        own_pub.as_bytes(),
        &ctx,
    )?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ct,
                aad: ctx.as_bytes(),
            },
        )
        .map_err(|_| Error::OpenFailed)?;
    Ok(Opened {
        kind,
        id: sealed.id,
        plaintext,
    })
}
