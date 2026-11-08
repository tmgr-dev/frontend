use crate::seal::parse_public;
use crate::{b64, b64_decode, h, hex, open, sign, verify};
use crate::{Dir, Entropy, Error, Kind, Result};
use hmac::{Hmac, KeyInit, Mac};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const MAX_TEXT_CHARS: usize = 1000;
pub const ISSUED_BACK_MS: i64 = 24 * 60 * 60 * 1000;
pub const ISSUED_AHEAD_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Copy, Debug)]
pub struct Pairing<'a> {
    pub mailbox: &'a str,
    pub desktop_fp: &'a str,
    pub phone_fp: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct HelloCtx<'a> {
    pub host: &'a str,
    pub user_id: i64,
    pub mailbox: &'a str,
    pub desktop_fp: &'a str,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HelloBody {
    pub sign_pub: String,
    pub kex_pub: String,
    pub name: String,
    pub next_token: String,
    pub mac: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PairOkBody {
    pub desktop_name: String,
    pub paired_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SnapshotCard {
    pub id: String,
    pub digest: String,
    pub state: String,
    pub session_gone: bool,
    pub projection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<CardLive>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct CardLive {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listening: Option<bool>,
    #[serde(default)]
    pub messages: Vec<LiveMessage>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LiveMessage {
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_ms: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SnapshotBody {
    pub v: u32,
    pub seq: i64,
    pub generated_at: i64,
    pub desktop_name: String,
    pub cards: Vec<SnapshotCard>,
    pub more: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Answer,
    Defer,
    Dismiss,
    Chat,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Answer => "answer",
            Action::Defer => "defer",
            Action::Dismiss => "dismiss",
            Action::Chat => "chat",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AnswerBody {
    pub card_id: String,
    pub card_digest: String,
    pub session: String,
    pub answer_id: String,
    pub issued_at: i64,
    pub action: Action,
    pub choice: Option<String>,
    pub text: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AckResult {
    Applied,
    Duplicate,
    Closed,
    Changed,
    Expired,
    Revoked,
    NotEligible,
    Invalid,
}

impl AckResult {
    pub fn as_str(self) -> &'static str {
        match self {
            AckResult::Applied => "applied",
            AckResult::Duplicate => "duplicate",
            AckResult::Closed => "closed",
            AckResult::Changed => "changed",
            AckResult::Expired => "expired",
            AckResult::Revoked => "revoked",
            AckResult::NotEligible => "not_eligible",
            AckResult::Invalid => "invalid",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AckBody {
    pub answer_id: String,
    pub result: AckResult,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Inner {
    body: String,
    sig: String,
}

#[derive(Clone, Debug)]
pub struct OpenedHello {
    pub body: HelloBody,
    pub phone_fp: String,
    pub code: String,
}

pub fn fingerprint(sign_pub: &[u8], kex_pub: &[u8]) -> String {
    h(&format!(
        "tmgr-phone-fp:v1:{}:{}",
        b64(sign_pub),
        b64(kex_pub)
    ))
}

pub fn digest(projection: &str) -> String {
    h(projection)
}

pub fn valid_answer_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn check_issued_at(issued_at: i64, now_ms: i64) -> Result<()> {
    if issued_at < now_ms - ISSUED_BACK_MS || issued_at > now_ms + ISSUED_AHEAD_MS {
        return Err(Error::BadWindow);
    }
    Ok(())
}

pub fn check_text(text: &str) -> Result<()> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(Error::TextTooLong);
    }
    Ok(())
}

pub fn hello_signed(c: &HelloCtx, body: &str) -> String {
    format!(
        "tmgr-phone-hello:v1:{}:{}:{}:{}:{}",
        c.host,
        c.user_id,
        c.mailbox,
        c.desktop_fp,
        h(body)
    )
}

pub fn pair_transcript(
    c: &HelloCtx,
    sign_pub_b64: &str,
    kex_pub_b64: &str,
    name: &str,
    next_token: &str,
) -> String {
    format!(
        "tmgr-phone-pair:v1:{}:{}:{}:{}:{sign_pub_b64}:{kex_pub_b64}:{}:{}",
        c.host,
        c.user_id,
        c.mailbox,
        c.desktop_fp,
        h(name),
        h(next_token)
    )
}

pub fn pair_mac(pair_secret: &[u8], transcript: &str) -> Result<String> {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(pair_secret).map_err(|_| Error::BadKey)?;
    mac.update(transcript.as_bytes());
    Ok(hex(&mac.finalize().into_bytes()))
}

pub fn pair_code(transcript: &str) -> String {
    let d = Sha256::digest(format!("tmgr-phone-code:v1:{transcript}").as_bytes());
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000;
    format!("{n:06}")
}

pub fn pair_ok_signed(p: &Pairing, body: &str) -> String {
    format!(
        "tmgr-phone-pair-ok:v1:{}:{}:{}:{}",
        p.mailbox,
        p.desktop_fp,
        p.phone_fp,
        h(body)
    )
}

pub fn snapshot_signed(p: &Pairing, seq: i64, generated_at: i64, body: &str) -> String {
    format!(
        "tmgr-phone-snapshot:v1:{}:{}:{}:{seq}:{generated_at}:{}",
        p.mailbox,
        p.desktop_fp,
        p.phone_fp,
        h(body)
    )
}

pub fn answer_signed(p: &Pairing, a: &AnswerBody) -> String {
    format!(
        "tmgr-decision-answer:v1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        p.mailbox,
        p.desktop_fp,
        p.phone_fp,
        a.card_id,
        a.card_digest,
        h(&a.session),
        a.answer_id,
        a.issued_at,
        a.action.as_str(),
        h(a.choice.as_deref().unwrap_or("")),
        h(a.text.as_deref().unwrap_or(""))
    )
}

pub fn ack_signed(p: &Pairing, answer_id: &str, result: AckResult, body: &str) -> String {
    format!(
        "tmgr-phone-ack:v1:{}:{}:{}:{answer_id}:{}:{}",
        p.mailbox,
        p.desktop_fp,
        p.phone_fp,
        result.as_str(),
        h(body)
    )
}

impl HelloBody {
    pub fn build(
        c: &HelloCtx,
        pair_secret: &[u8],
        sign_pub: &[u8],
        kex_pub: &[u8],
        name: &str,
        next_token: &str,
    ) -> Result<HelloBody> {
        let (s, k) = (b64(sign_pub), b64(kex_pub));
        let mac = pair_mac(pair_secret, &pair_transcript(c, &s, &k, name, next_token))?;
        Ok(HelloBody {
            sign_pub: s,
            kex_pub: k,
            name: name.to_string(),
            next_token: next_token.to_string(),
            mac,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn seal_signed(
    recipient_kex_pub: &[u8],
    mailbox: &str,
    dir: Dir,
    kind: Kind,
    id: &str,
    sign_secret: &[u8],
    body: String,
    signed: &str,
    ent: &Entropy,
) -> Result<String> {
    let sig = sign(sign_secret, signed)?;
    let plaintext = serde_json::to_vec(&Inner { body, sig }).map_err(|_| Error::BadJson)?;
    let sealed = crate::seal_with(recipient_kex_pub, mailbox, dir, kind, id, &plaintext, ent)?;
    serde_json::to_string(&sealed).map_err(|_| Error::BadJson)
}

fn to_body<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|_| Error::BadJson)
}

#[allow(clippy::too_many_arguments)]
fn open_signed<T: DeserializeOwned>(
    kex_secret: &[u8],
    mailbox: &str,
    dir: Dir,
    sealed_json: &str,
    kind: Kind,
    signer: impl FnOnce(&T) -> Result<Vec<u8>>,
    outer_id: impl FnOnce(&T) -> String,
    signed: impl FnOnce(&T, &str) -> String,
) -> Result<T> {
    let opened = open(kex_secret, mailbox, dir, sealed_json, &[kind])?;
    let inner: Inner = serde_json::from_slice(&opened.plaintext).map_err(|_| Error::BadJson)?;
    let body: T = serde_json::from_str(&inner.body).map_err(|_| Error::BadJson)?;
    if outer_id(&body) != opened.id {
        return Err(Error::IdMismatch);
    }
    let signer_pub = signer(&body)?;
    verify(&signer_pub, &signed(&body, &inner.body), &inner.sig)?;
    Ok(body)
}

#[allow(clippy::too_many_arguments)]
pub fn seal_hello_with(
    desktop_kex_pub: &[u8],
    c: &HelloCtx,
    phone_sign_secret: &[u8],
    body: &HelloBody,
    ent: &Entropy,
) -> Result<String> {
    let text = to_body(body)?;
    let signed = hello_signed(c, &text);
    seal_signed(
        desktop_kex_pub,
        c.mailbox,
        Dir::P2d,
        Kind::Hello,
        "hello",
        phone_sign_secret,
        text,
        &signed,
        ent,
    )
}

pub fn seal_hello(
    desktop_kex_pub: &[u8],
    c: &HelloCtx,
    phone_sign_secret: &[u8],
    body: &HelloBody,
) -> Result<String> {
    seal_hello_with(
        desktop_kex_pub,
        c,
        phone_sign_secret,
        body,
        &Entropy::random()?,
    )
}

pub fn open_hello(
    desktop_kex_secret: &[u8],
    c: &HelloCtx,
    pair_secret: &[u8],
    sealed_json: &str,
) -> Result<OpenedHello> {
    let body: HelloBody = open_signed(
        desktop_kex_secret,
        c.mailbox,
        Dir::P2d,
        sealed_json,
        Kind::Hello,
        |b: &HelloBody| b64_decode(&b.sign_pub),
        |_| "hello".to_string(),
        |_, text| hello_signed(c, text),
    )?;
    let kex = b64_decode(&body.kex_pub)?;
    parse_public(&kex)?;
    let transcript = pair_transcript(
        c,
        &body.sign_pub,
        &body.kex_pub,
        &body.name,
        &body.next_token,
    );
    let expected = pair_mac(pair_secret, &transcript)?;
    if !bool::from(expected.as_bytes().ct_eq(body.mac.as_bytes())) {
        return Err(Error::BadMac);
    }
    let phone_fp = fingerprint(&b64_decode(&body.sign_pub)?, &kex);
    Ok(OpenedHello {
        code: pair_code(&transcript),
        phone_fp,
        body,
    })
}

pub fn seal_pair_ok_with(
    phone_kex_pub: &[u8],
    p: &Pairing,
    desktop_sign_secret: &[u8],
    body: &PairOkBody,
    ent: &Entropy,
) -> Result<String> {
    let text = to_body(body)?;
    let signed = pair_ok_signed(p, &text);
    seal_signed(
        phone_kex_pub,
        p.mailbox,
        Dir::D2p,
        Kind::PairOk,
        "pair_ok",
        desktop_sign_secret,
        text,
        &signed,
        ent,
    )
}

pub fn seal_pair_ok(
    phone_kex_pub: &[u8],
    p: &Pairing,
    desktop_sign_secret: &[u8],
    body: &PairOkBody,
) -> Result<String> {
    seal_pair_ok_with(
        phone_kex_pub,
        p,
        desktop_sign_secret,
        body,
        &Entropy::random()?,
    )
}

pub fn open_pair_ok(
    phone_kex_secret: &[u8],
    p: &Pairing,
    desktop_sign_pub: &[u8],
    sealed_json: &str,
) -> Result<PairOkBody> {
    open_signed(
        phone_kex_secret,
        p.mailbox,
        Dir::D2p,
        sealed_json,
        Kind::PairOk,
        |_| Ok(desktop_sign_pub.to_vec()),
        |_| "pair_ok".to_string(),
        |_, text| pair_ok_signed(p, text),
    )
}

pub fn seal_snapshot_with(
    phone_kex_pub: &[u8],
    p: &Pairing,
    desktop_sign_secret: &[u8],
    body: &SnapshotBody,
    ent: &Entropy,
) -> Result<String> {
    let text = to_body(body)?;
    let signed = snapshot_signed(p, body.seq, body.generated_at, &text);
    seal_signed(
        phone_kex_pub,
        p.mailbox,
        Dir::D2p,
        Kind::Snapshot,
        &body.seq.to_string(),
        desktop_sign_secret,
        text,
        &signed,
        ent,
    )
}

pub fn seal_snapshot(
    phone_kex_pub: &[u8],
    p: &Pairing,
    desktop_sign_secret: &[u8],
    body: &SnapshotBody,
) -> Result<String> {
    seal_snapshot_with(
        phone_kex_pub,
        p,
        desktop_sign_secret,
        body,
        &Entropy::random()?,
    )
}

pub fn open_snapshot(
    phone_kex_secret: &[u8],
    p: &Pairing,
    desktop_sign_pub: &[u8],
    sealed_json: &str,
) -> Result<SnapshotBody> {
    let body: SnapshotBody = open_signed(
        phone_kex_secret,
        p.mailbox,
        Dir::D2p,
        sealed_json,
        Kind::Snapshot,
        |_| Ok(desktop_sign_pub.to_vec()),
        |b: &SnapshotBody| b.seq.to_string(),
        |b, text| snapshot_signed(p, b.seq, b.generated_at, text),
    )?;
    if body.v != 1 || body.cards.iter().any(|c| digest(&c.projection) != c.digest) {
        return Err(Error::BadFormat);
    }
    Ok(body)
}

pub fn seal_answer_with(
    desktop_kex_pub: &[u8],
    p: &Pairing,
    phone_sign_secret: &[u8],
    body: &AnswerBody,
    ent: &Entropy,
) -> Result<String> {
    if !valid_answer_id(&body.answer_id) {
        return Err(Error::BadFormat);
    }
    let text = to_body(body)?;
    let signed = answer_signed(p, body);
    seal_signed(
        desktop_kex_pub,
        p.mailbox,
        Dir::P2d,
        Kind::Answer,
        &body.answer_id,
        phone_sign_secret,
        text,
        &signed,
        ent,
    )
}

pub fn seal_answer(
    desktop_kex_pub: &[u8],
    p: &Pairing,
    phone_sign_secret: &[u8],
    body: &AnswerBody,
) -> Result<String> {
    seal_answer_with(
        desktop_kex_pub,
        p,
        phone_sign_secret,
        body,
        &Entropy::random()?,
    )
}

pub fn open_answer(
    desktop_kex_secret: &[u8],
    p: &Pairing,
    phone_sign_pub: &[u8],
    sealed_json: &str,
) -> Result<AnswerBody> {
    let body: AnswerBody = open_signed(
        desktop_kex_secret,
        p.mailbox,
        Dir::P2d,
        sealed_json,
        Kind::Answer,
        |_| Ok(phone_sign_pub.to_vec()),
        |b: &AnswerBody| b.answer_id.clone(),
        |b, _| answer_signed(p, b),
    )?;
    if !valid_answer_id(&body.answer_id) {
        return Err(Error::BadFormat);
    }
    Ok(body)
}

pub fn seal_ack_with(
    phone_kex_pub: &[u8],
    p: &Pairing,
    desktop_sign_secret: &[u8],
    body: &AckBody,
    ent: &Entropy,
) -> Result<String> {
    let text = to_body(body)?;
    let signed = ack_signed(p, &body.answer_id, body.result, &text);
    seal_signed(
        phone_kex_pub,
        p.mailbox,
        Dir::D2p,
        Kind::Ack,
        &body.answer_id,
        desktop_sign_secret,
        text,
        &signed,
        ent,
    )
}

pub fn seal_ack(
    phone_kex_pub: &[u8],
    p: &Pairing,
    desktop_sign_secret: &[u8],
    body: &AckBody,
) -> Result<String> {
    seal_ack_with(
        phone_kex_pub,
        p,
        desktop_sign_secret,
        body,
        &Entropy::random()?,
    )
}

pub fn open_ack(
    phone_kex_secret: &[u8],
    p: &Pairing,
    desktop_sign_pub: &[u8],
    sealed_json: &str,
) -> Result<AckBody> {
    open_signed(
        phone_kex_secret,
        p.mailbox,
        Dir::D2p,
        sealed_json,
        Kind::Ack,
        |_| Ok(desktop_sign_pub.to_vec()),
        |b: &AckBody| b.answer_id.clone(),
        |b, text| ack_signed(p, &b.answer_id, b.result, text),
    )
}
