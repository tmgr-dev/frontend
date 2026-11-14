use std::fmt;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Serialize;
use subtle::ConstantTimeEq;
use tmgr_phone::{HelloCtx, OpenedHello, PairOkBody, Pairing};

use super::pins::{Pin, MAX_PHONES};
use super::relay::{self, Conn};
use super::{PairingView, Phones};

pub const QR_TTL_MS: i64 = 5 * 60 * 1000;
pub const CODE_ATTEMPTS: u8 = 3;
const NAME_MAX: usize = 64;
const SEEN_MAX: usize = 100;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PairState {
  Waiting,
  Confirm,
  Failed,
  Done,
}

pub struct Pending {
  pub(super) api_base: String,
  pub(super) user_id: i64,
  pub(super) conn: Conn,
  pub(super) desktop_name: String,
  pub(super) sign_secret: Vec<u8>,
  pub(super) kex_secret: Vec<u8>,
  pub(super) desktop_fp: String,
  pub(super) secret: Vec<u8>,
  pub(super) exp_ms: i64,
  pub(super) qr: String,
  pub(super) state: PairState,
  pub(super) hello: Option<(OpenedHello, String)>,
  pub(super) seen: Vec<String>,
  pub(super) attempts_left: u8,
  pub(super) error: Option<String>,
  pub(super) finishing: bool,
}

impl fmt::Debug for Pending {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Pending")
      .field("mailbox", &self.conn.mailbox)
      .field("state", &self.state)
      .field("secrets", &"<redacted>")
      .finish_non_exhaustive()
  }
}

#[derive(Serialize)]
struct QrPayload<'a> {
  v: u32,
  host: &'a str,
  user_id: i64,
  mailbox: &'a str,
  phone_token: &'a str,
  d_sign: String,
  d_kex: String,
  secret: String,
  name: &'a str,
  exp: i64,
}

fn secret_key() -> Result<Vec<u8>, String> {
  loop {
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).map_err(|e| e.to_string())?;
    if tmgr_phone::public_key(&k).is_ok() {
      return Ok(k.to_vec());
    }
  }
}

fn clean_name(raw: &str) -> String {
  raw.chars().filter(|c| !c.is_control()).take(NAME_MAX).collect::<String>().trim().to_owned()
}

fn url_safe(s: &str, max: usize) -> bool {
  !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_uuid(s: &str) -> bool {
  s.len() == 36
    && s.bytes().enumerate().all(|(i, b)| match i {
      8 | 13 | 18 | 23 => b == b'-',
      _ => b.is_ascii_hexdigit(),
    })
}

pub(super) fn pair_link(json: &[u8]) -> String {
  format!("tmgr://pair#tmgr-pair:v1:{}", URL_SAFE_NO_PAD.encode(json))
}

pub(super) fn qr_data_url(text: &str) -> Result<String, String> {
  let code = qrcode::QrCode::new(text.as_bytes()).map_err(|_| "could not draw the QR code".to_owned())?;
  let svg = code.render::<qrcode::render::svg::Color>().min_dimensions(320, 320).quiet_zone(true).build();
  Ok(format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg)))
}

impl Phones {
  pub fn begin_pairing(
    &self,
    api_base: &str,
    user_id: i64,
    mailbox_id: &str,
    desktop_token: &str,
    phone_token: &str,
    desktop_name: &str,
  ) -> Result<(), String> {
    let base = crate::local_move::trusted_api_base(api_base, crate::local_move::EXTRA_API)?;
    if !is_uuid(mailbox_id) || !url_safe(desktop_token, 128) || !url_safe(phone_token, 128) {
      return Err("the relay mailbox data is not valid".into());
    }
    let name = clean_name(desktop_name);
    if name.is_empty() {
      return Err("the desktop needs a name".into());
    }
    let now = (self.now)();
    let live = self.pins.list()?.into_iter().filter(|p| !p.expired(now)).count();
    if live >= MAX_PHONES {
      return Err(format!("{MAX_PHONES} phones are already paired. Revoke one first."));
    }
    if self.pins.get(mailbox_id)?.is_some() {
      return Err("that mailbox is already paired".into());
    }
    self.cancel();
    let sign_secret = secret_key()?;
    let kex_secret = secret_key()?;
    let d_sign = tmgr_phone::public_key(&sign_secret).map_err(|e| e.to_string())?;
    let d_kex = tmgr_phone::public_key(&kex_secret).map_err(|e| e.to_string())?;
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| e.to_string())?;
    let exp_ms = now + QR_TTL_MS;
    let payload = QrPayload {
      v: 1,
      host: &base,
      user_id,
      mailbox: mailbox_id,
      phone_token,
      d_sign: tmgr_phone::b64(&d_sign),
      d_kex: tmgr_phone::b64(&d_kex),
      secret: tmgr_phone::b64(&secret),
      name: &name,
      exp: exp_ms,
    };
    let json = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    let text = pair_link(&json);
    let qr = qr_data_url(&text)?;
    *self.pairing.lock().unwrap_or_else(|p| p.into_inner()) = Some(Pending {
      api_base: base.clone(),
      user_id,
      conn: Conn { api_base: base, mailbox: mailbox_id.to_owned(), token: desktop_token.to_owned() },
      desktop_name: name,
      desktop_fp: tmgr_phone::fingerprint(&d_sign, &d_kex),
      sign_secret,
      kex_secret,
      secret: secret.to_vec(),
      exp_ms,
      qr,
      state: PairState::Waiting,
      hello: None,
      seen: Vec::new(),
      attempts_left: CODE_ATTEMPTS,
      error: None,
      finishing: false,
    });
    log::info!("[phones] pairing started");
    self.host.changed();
    self.host.open_window();
    self.wake();
    Ok(())
  }

  pub fn pairing_active(&self) -> bool {
    self
      .pairing
      .lock()
      .unwrap_or_else(|p| p.into_inner())
      .as_ref()
      .is_some_and(|p| matches!(p.state, PairState::Waiting | PairState::Confirm))
  }

  pub fn pairing_view(&self) -> Option<PairingView> {
    self.expire_pairing();
    let g = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
    let p = g.as_ref()?;
    Some(PairingView {
      state: p.state,
      qr_svg_data_url: if p.state == PairState::Waiting { p.qr.clone() } else { String::new() },
      expires_ms: p.exp_ms,
      attempts_left: p.attempts_left,
      phone_name: p.hello.as_ref().map(|(h, _)| clean_name(&h.body.name)),
      error: p.error.clone(),
    })
  }

  fn fail(&self, p: &mut Pending, reason: &str) -> Conn {
    p.state = PairState::Failed;
    p.error = Some(reason.to_owned());
    p.hello = None;
    self.audit.append((self.now)(), "pair_failed", &[("mailbox", &p.conn.mailbox)]);
    p.conn.clone()
  }

  pub fn expire_pairing(&self) {
    let now = (self.now)();
    let conn = {
      let mut g = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
      match g.as_mut() {
        Some(p) if matches!(p.state, PairState::Waiting | PairState::Confirm) && !p.finishing && now >= p.exp_ms => {
          Some(self.fail(p, "The QR code expired. Start again."))
        }
        _ => None,
      }
    };
    if let Some(conn) = conn {
      log::info!("[phones] pairing: the QR code expired");
      let _ = self.relay.delete_mailbox(&conn);
      self.host.changed();
    }
  }

  pub fn poll_pairing(&self) {
    self.expire_pairing();
    let (conn, kex, ctx_base, secret) = {
      let g = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
      match g.as_ref() {
        Some(p) if p.state == PairState::Waiting || p.state == PairState::Confirm => (
          p.conn.clone(),
          p.kex_secret.clone(),
          (p.api_base.clone(), p.user_id, p.desktop_fp.clone()),
          p.secret.clone(),
        ),
        _ => return,
      }
    };
    let msgs = match self.relay.list_messages(&conn) {
      Ok(m) => {
        self.poll_recovered("pairing");
        m
      }
      Err(e) => {
        self.poll_failed("pairing", "pairing: reading the relay messages failed", &e);
        return;
      }
    };
    let mut junk = Vec::new();
    let mut abort = None;
    let mut changed = false;
    {
      let mut g = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
      let Some(p) = g.as_mut() else { return };
      if p.conn.mailbox != conn.mailbox || p.finishing || !matches!(p.state, PairState::Waiting | PairState::Confirm) {
        return;
      }
      for m in relay::usable(msgs).filter(|m| m.kind == "hello") {
        if p.seen.contains(&m.id) {
          continue;
        }
        let ctx = HelloCtx { host: &ctx_base.0, user_id: ctx_base.1, mailbox: &conn.mailbox, desktop_fp: &ctx_base.2 };
        let opened = tmgr_phone::open_hello(&kex, &ctx, &secret, &m.body);
        if opened.is_ok() && p.seen.len() < SEEN_MAX {
          p.seen.push(m.id.clone());
        }
        match opened {
          Err(e) => {
            log::warn!("[phones] pairing: a message was not a valid hello and is deleted: {e}");
            junk.push(m.clone());
          }
          Ok(h) => match &p.hello {
            None => {
              p.hello = Some((h, m.id.clone()));
              p.state = PairState::Confirm;
              log::info!("[phones] pairing: the phone answered, waiting for the code");
              changed = true;
            }
            Some((first, _)) if first.body == h.body => {}
            Some(_) => {
              log::warn!("[phones] pairing: a second device answered the QR code");
              abort = Some(self.fail(p, "A second device answered the QR code. Pairing was cancelled."));
              changed = true;
            }
          },
        }
      }
    }
    for m in junk {
      let _ = self.relay.delete_message(&conn, &m.id);
    }
    if let Some(conn) = abort {
      let _ = self.relay.delete_mailbox(&conn);
    }
    if changed {
      self.host.changed();
    }
  }

  pub fn confirm(&self, code: &str) -> Result<(), String> {
    self.expire_pairing();
    let typed: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
    let go = {
      let mut g = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
      let p = g.as_mut().ok_or_else(|| "no pairing is in progress".to_owned())?;
      if p.state != PairState::Confirm || p.finishing {
        return Err("the phone has not answered yet".into());
      }
      let (hello, _) = p.hello.as_ref().ok_or_else(|| "the phone has not answered yet".to_owned())?;
      let matches: bool = hello.code.as_bytes().ct_eq(typed.as_bytes()).into();
      if !matches {
        p.attempts_left = p.attempts_left.saturating_sub(1);
        log::info!("[phones] pairing: wrong code, {} attempts left", p.attempts_left);
        let dead = (p.attempts_left == 0).then(|| self.fail(p, "Too many wrong codes. Start again."));
        drop(g);
        if let Some(conn) = dead {
          let _ = self.relay.delete_mailbox(&conn);
        }
        self.host.changed();
        return Ok(());
      }
      p.finishing = true;
      let (hello, msg_id) = p.hello.clone().ok_or_else(|| "the phone has not answered yet".to_owned())?;
      (
        hello,
        msg_id,
        p.conn.clone(),
        p.sign_secret.clone(),
        p.kex_secret.clone(),
        p.desktop_fp.clone(),
        p.desktop_name.clone(),
        p.api_base.clone(),
        p.user_id,
      )
    };
    let (hello, msg_id, conn, sign_secret, kex_secret, desktop_fp, desktop_name, api_base, user_id) = go;
    let now = (self.now)();
    let finished = self.finish_pairing(
      &hello,
      &conn,
      &sign_secret,
      &kex_secret,
      &desktop_fp,
      &desktop_name,
      &api_base,
      user_id,
      now,
    );
    let mut g = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
    let Some(p) = g.as_mut() else { return Ok(()) };
    p.finishing = false;
    match finished {
      Ok(()) => {
        p.state = PairState::Done;
        log::info!("[phones] pairing: done");
        drop(g);
        let _ = self.relay.delete_message(&conn, &msg_id);
        self.host.changed();
        self.refresh_snapshots(true);
      }
      Err(reason) => {
        log::warn!("[phones] pairing: failed: {reason}");
        let conn = self.fail(p, &reason);
        drop(g);
        let _ = self.relay.delete_mailbox(&conn);
        self.host.changed();
      }
    }
    Ok(())
  }

  #[allow(clippy::too_many_arguments)]
  fn finish_pairing(
    &self,
    hello: &OpenedHello,
    conn: &Conn,
    sign_secret: &[u8],
    kex_secret: &[u8],
    desktop_fp: &str,
    desktop_name: &str,
    api_base: &str,
    user_id: i64,
    now: i64,
  ) -> Result<(), String> {
    let phone_kex = tmgr_phone::b64_decode(&hello.body.kex_pub).map_err(|e| e.to_string())?;
    let pairing = Pairing { mailbox: &conn.mailbox, desktop_fp, phone_fp: &hello.phone_fp };
    let ok = PairOkBody { desktop_name: desktop_name.to_owned(), paired_at: now };
    let sealed = tmgr_phone::seal_pair_ok(&phone_kex, &pairing, sign_secret, &ok).map_err(|e| e.to_string())?;
    let unreachable = |_| "Could not reach the server to finish pairing. Start again.".to_owned();
    self.relay.post_message(conn, "pair_ok", "pair_ok", &sealed).map_err(unreachable)?;
    self.relay.put_phone_token(conn, &hello.body.next_token).map_err(unreachable)?;
    let pin = Pin {
      mailbox: conn.mailbox.clone(),
      api_base: api_base.to_owned(),
      user_id,
      desktop_sign_secret: tmgr_phone::b64(sign_secret),
      desktop_kex_secret: tmgr_phone::b64(kex_secret),
      desktop_fp: desktop_fp.to_owned(),
      desktop_token: conn.token.clone(),
      desktop_name: desktop_name.to_owned(),
      phone_sign_pub: hello.body.sign_pub.clone(),
      phone_kex_pub: hello.body.kex_pub.clone(),
      phone_fp: hello.phone_fp.clone(),
      phone_name: clean_name(&hello.body.name),
      last_seq: 0,
      paired_ms: now,
      last_used_ms: 0,
    };
    self.pins.put(&pin).map_err(|_| "Could not save the pairing in the keychain.".to_owned())?;
    self.audit.append(now, "pair", &[("mailbox", &conn.mailbox), ("phone_fp", &hello.phone_fp)]);
    Ok(())
  }

  pub fn cancel(&self) {
    let taken = self.pairing.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(p) = taken {
      if matches!(p.state, PairState::Waiting | PairState::Confirm) {
        log::info!("[phones] pairing: cancelled");
        let _ = self.relay.delete_mailbox(&p.conn);
        self.audit.append((self.now)(), "pair_cancelled", &[("mailbox", &p.conn.mailbox)]);
      }
      self.host.changed();
    }
  }
}
