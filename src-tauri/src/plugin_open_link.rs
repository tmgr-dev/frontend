use tauri::{Manager, Runtime, Window};
use tauri_plugin_opener::OpenerExt;

const MAX_LEN: usize = 2000;

#[derive(Clone, Copy)]
enum Kind {
  Digits,
  Username,
  Invite,
  Timestamp,
  Flag,
}

const MESSAGE_PARAMS: [(&str, Kind); 5] = [
  ("post", Kind::Digits),
  ("single", Kind::Flag),
  ("thread", Kind::Digits),
  ("comment", Kind::Digits),
  ("t", Kind::Timestamp),
];

struct Action {
  params: &'static [(&'static str, Kind)],
  with_message_params: bool,
  required: &'static [&'static [&'static str]],
}

fn action(name: &str) -> Option<Action> {
  Some(match name {
    "resolve" => Action {
      params: &[("domain", Kind::Username), ("profile", Kind::Flag)],
      with_message_params: true,
      required: &[&["domain"]],
    },
    "privatepost" => Action {
      params: &[("channel", Kind::Digits)],
      with_message_params: true,
      required: &[&["channel"], &["post"]],
    },
    "join" => Action {
      params: &[("invite", Kind::Invite)],
      with_message_params: false,
      required: &[&["invite"]],
    },
    _ => return None,
  })
}

fn value_ok(kind: Kind, value: &str) -> bool {
  let all = |ok: fn(char) -> bool| value.chars().all(ok);
  let word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
  match kind {
    Kind::Digits => (1..=20).contains(&value.len()) && all(|c| c.is_ascii_digit()),
    Kind::Timestamp => (1..=10).contains(&value.len()) && all(|c| c.is_ascii_digit()),
    Kind::Username => {
      (3..=32).contains(&value.len())
        && value.starts_with(|c: char| c.is_ascii_alphabetic())
        && all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
    Kind::Invite => (8..=64).contains(&value.len()) && value.chars().all(word),
    Kind::Flag => false,
  }
}

/// `tg://<action>?<query>` with a known action, known keys, plain values, no userinfo, port, path or fragment.
/// Must stay in step with `isTgLink` in src/pluginSystem/uiTree.ts.
pub fn validate_tg_link(url: &str) -> Result<(), String> {
  let bad = || "not an allowed Telegram link".to_string();
  if url.len() > MAX_LEN || url.is_empty() || !url.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
    return Err(bad());
  }
  let rest = url.strip_prefix("tg://").ok_or_else(bad)?;
  let (name, query) = rest.split_once('?').ok_or_else(bad)?;
  if query.contains(['?', '#']) || name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase()) {
    return Err(bad());
  }
  let action = action(name).ok_or_else(bad)?;
  let mut seen: Vec<&str> = Vec::new();
  for pair in query.split('&') {
    let (key, value) = match pair.split_once('=') {
      Some((k, v)) => (k, Some(v)),
      None => (pair, None),
    };
    let kind = action
      .params
      .iter()
      .chain(if action.with_message_params {
        MESSAGE_PARAMS.iter()
      } else {
        [].iter()
      })
      .find(|(k, _)| *k == key)
      .map(|(_, kind)| *kind)
      .ok_or_else(bad)?;
    if seen.contains(&key) {
      return Err(bad());
    }
    seen.push(key);
    match (kind, value) {
      (Kind::Flag, None) => {}
      (Kind::Flag, Some(_)) | (_, None) => return Err(bad()),
      (kind, Some(v)) if value_ok(kind, v) => {}
      _ => return Err(bad()),
    }
  }
  if action.required.iter().all(|keys| keys.iter().any(|k| seen.contains(k))) {
    Ok(())
  } else {
    Err(bad())
  }
}

/// Hands one validated Telegram deep link to the system opener. Main window only: a plugin has no way to call this.
#[tauri::command]
pub fn plugin_open_tg_link<R: Runtime>(window: Window<R>, url: String) -> Result<(), String> {
  if window.label() != "main" {
    return Err("not allowed from this window".into());
  }
  validate_tg_link(&url)?;
  window
    .app_handle()
    .opener()
    .open_url(&url, None::<&str>)
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::validate_tg_link;

  #[test]
  fn allows_chat_and_post_links() {
    for url in [
      "tg://resolve?domain=telegram",
      "tg://resolve?domain=durov&post=12&single&thread=3&comment=4&t=90",
      "tg://resolve?domain=telegram&profile",
      "tg://privatepost?channel=1234567890&post=7",
      "tg://join?invite=AbCdEfGh12_-",
    ] {
      assert!(validate_tg_link(url).is_ok(), "{url}");
    }
  }

  #[test]
  fn refuses_other_actions_and_schemes() {
    for url in [
      "tg://proxy?server=a&port=1&secret=b",
      "tg://socks?server=a&port=1",
      "tg://login?code=1",
      "tg://passport?bot_id=1",
      "tg://setlanguage?lang=en",
      "tg://addstickers?set=x",
      "tg://addtheme?slug=x",
      "tg://settings",
      "tg://confirmphone?phone=1&hash=2",
      "tg://bg?color=ffffff",
      "tg://msg_url?url=https://a.b",
      "tg://oauth?token=x",
      "TG://resolve?domain=telegram",
      "Tg://resolve?domain=telegram",
      "tg:resolve?domain=telegram",
      "https://t.me/telegram",
      "javascript:alert(1)",
      "file:///etc/passwd",
      "tmgr://resolve?domain=telegram",
      "",
    ] {
      assert!(validate_tg_link(url).is_err(), "{url}");
    }
  }

  #[test]
  fn refuses_state_changing_and_unknown_params() {
    for url in [
      "tg://resolve?domain=bot&startgroup=x&admin=change_info",
      "tg://resolve?domain=bot&startchannel&admin=post_messages",
      "tg://resolve?domain=bot&startapp=x",
      "tg://resolve?domain=bot&phone=123",
      "tg://resolve?domain=bot&videochat",
      "tg://resolve?domain=bot&text=hello",
      "tg://resolve?domain=bot&post=1&post=2",
      "tg://resolve?domain=bot&profile=1",
      "tg://resolve?domain=bot&start",
      "tg://resolve?domain=bot&start=abc",
      "tg://openmessage?user_id=42&message_id=9",
      "tg://openmessage?chat_id=42",
      "tg://resolve?domain=bot&__proto__=1",
    ] {
      assert!(validate_tg_link(url).is_err(), "{url}");
    }
  }

  #[test]
  fn refuses_malformed_urls() {
    for url in [
      "tg://resolve",
      "tg://resolve?",
      "tg://resolve?domain=",
      "tg://resolve?domain=a",
      "tg://resolve?domain=tele gram",
      "tg://resolve?domain=telegram\n",
      "tg://resolve?domain=telegram#x",
      "tg://resolve/x?domain=telegram",
      "tg://user@resolve?domain=telegram",
      "tg://resolve:80?domain=telegram",
      "tg://resolve?domain=%74elegram",
      "tg://resolve?domain=telegram&post=1x",
      "tg://privatepost?channel=1",
      "tg://privatepost?post=1",
      "tg://join?invite=short",
      "tg://resolve?domain=\u{442}elegram",
    ] {
      assert!(validate_tg_link(url).is_err(), "{url}");
    }
    let long = format!("tg://resolve?domain=telegram&post={}", "a".repeat(2000));
    assert!(validate_tg_link(&long).is_err());
  }
}
