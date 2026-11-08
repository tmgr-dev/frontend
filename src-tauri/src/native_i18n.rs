use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
  En,
  Ru,
  Es,
  ZhCn,
  PtBr,
  De,
  Fr,
  Ja,
  Ko,
  Pl,
}

pub const LOCALES: [Locale; 10] = [
  Locale::En,
  Locale::Ru,
  Locale::Es,
  Locale::ZhCn,
  Locale::PtBr,
  Locale::De,
  Locale::Fr,
  Locale::Ja,
  Locale::Ko,
  Locale::Pl,
];

impl Locale {
  pub fn tag(self) -> &'static str {
    match self {
      Locale::En => "en",
      Locale::Ru => "ru",
      Locale::Es => "es",
      Locale::ZhCn => "zh-CN",
      Locale::PtBr => "pt-BR",
      Locale::De => "de",
      Locale::Fr => "fr",
      Locale::Ja => "ja",
      Locale::Ko => "ko",
      Locale::Pl => "pl",
    }
  }
}

/// Mirrors `normalizeLocale` in `src/i18n/locale.ts`; unknown tags fall back to English.
pub fn normalize(tag: &str) -> Locale {
  let lowered = tag.trim().replace('_', "-").to_lowercase();
  let parts: Vec<&str> = lowered.split('-').collect();
  let two = parts.iter().take(2).copied().collect::<Vec<_>>().join("-");
  if let Some(exact) = LOCALES.iter().find(|l| l.tag().to_lowercase() == two) {
    return *exact;
  }
  match parts[0] {
    "zh" if parts.iter().any(|p| matches!(*p, "hant" | "tw" | "hk" | "mo")) => Locale::En,
    "zh" => Locale::ZhCn,
    "pt" => Locale::PtBr,
    language => LOCALES.iter().find(|l| l.tag() == language).copied().unwrap_or(Locale::En),
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
  StopTimer,
  SwitchTo,
  DoNotDisturb,
  DndOff,
  Dnd1h,
  Dnd3h,
  DndTomorrow,
  OpenApp,
  Decisions,
  Shortcuts,
  LaunchAtLogin,
  CheckForUpdates,
  Quit,
  QuickAdd,
  DecisionsWindow,
  PairedPhones,
  AgentTerminal,
  UnlinkProfile,
  Back,
  Forward,
}

pub const KEY_COUNT: usize = 20;

type Table = [&'static str; KEY_COUNT];

const EN: Table = [
  "Stop timer",
  "Switch to",
  "Do Not Disturb",
  "Off",
  "For 1 hour",
  "For 3 hours",
  "Until tomorrow",
  "Open TMGR",
  "Decisions ({n})",
  "Shortcuts…",
  "Launch at Login",
  "Check for Updates…",
  "Quit TMGR",
  "Quick add",
  "Decisions · Doom Solving",
  "Paired phones",
  "Agent terminal",
  "Unlink local profile",
  "Back",
  "Forward",
];

const RU: Table = [
  "Остановить таймер",
  "Переключиться на",
  "Не беспокоить",
  "Выкл.",
  "На 1 час",
  "На 3 часа",
  "До завтра",
  "Открыть TMGR",
  "Решения ({n})",
  "Быстрые клавиши…",
  "Запускать при входе",
  "Проверить обновления…",
  "Выйти из TMGR",
  "Быстрое добавление",
  "Решения · Doom Solving",
  "Сопряжённые телефоны",
  "Терминал агента",
  "Отвязать локальный профиль",
  "Назад",
  "Вперёд",
];

const ES: Table = [
  "Detener temporizador",
  "Cambiar a",
  "No molestar",
  "Desactivado",
  "Durante 1 hora",
  "Durante 3 horas",
  "Hasta mañana",
  "Abrir TMGR",
  "Decisiones ({n})",
  "Atajos…",
  "Abrir al iniciar sesión",
  "Buscar actualizaciones…",
  "Salir de TMGR",
  "Añadir rápido",
  "Decisiones · Doom Solving",
  "Teléfonos vinculados",
  "Terminal del agente",
  "Desvincular perfil local",
  "Atrás",
  "Adelante",
];

const ZH_CN: Table = [
  "停止计时器",
  "切换到",
  "勿扰模式",
  "关闭",
  "1 小时",
  "3 小时",
  "到明天",
  "打开 TMGR",
  "决策 ({n})",
  "快捷键…",
  "登录时启动",
  "检查更新…",
  "退出 TMGR",
  "快速添加",
  "决策 · Doom Solving",
  "已配对的手机",
  "智能体终端",
  "取消关联本地个人资料",
  "后退",
  "前进",
];

const PT_BR: Table = [
  "Parar cronômetro",
  "Alternar para",
  "Não perturbe",
  "Desativado",
  "Por 1 hora",
  "Por 3 horas",
  "Até amanhã",
  "Abrir o TMGR",
  "Decisões ({n})",
  "Atalhos…",
  "Abrir ao fazer login",
  "Verificar atualizações…",
  "Sair do TMGR",
  "Adição rápida",
  "Decisões · Doom Solving",
  "Telefones pareados",
  "Terminal do agente",
  "Desvincular perfil local",
  "Voltar",
  "Avançar",
];

const DE: Table = [
  "Timer stoppen",
  "Wechseln zu",
  "Nicht stören",
  "Aus",
  "1 Stunde lang",
  "3 Stunden lang",
  "Bis morgen",
  "TMGR öffnen",
  "Entscheidungen ({n})",
  "Tastenkürzel…",
  "Beim Anmelden starten",
  "Nach Updates suchen…",
  "TMGR beenden",
  "Schnell hinzufügen",
  "Entscheidungen · Doom Solving",
  "Gekoppelte Telefone",
  "Agent-Terminal",
  "Lokales Profil trennen",
  "Zurück",
  "Vorwärts",
];

const FR: Table = [
  "Arrêter le minuteur",
  "Passer à",
  "Ne pas déranger",
  "Désactivé",
  "Pendant 1 heure",
  "Pendant 3 heures",
  "Jusqu'à demain",
  "Ouvrir TMGR",
  "Décisions ({n})",
  "Raccourcis…",
  "Lancer à la connexion",
  "Rechercher les mises à jour…",
  "Quitter TMGR",
  "Ajout rapide",
  "Décisions · Doom Solving",
  "Téléphones associés",
  "Terminal de l'agent",
  "Dissocier le profil local",
  "Précédent",
  "Suivant",
];

const JA: Table = [
  "タイマーを停止",
  "切り替え先",
  "おやすみモード",
  "オフ",
  "1時間",
  "3時間",
  "明日まで",
  "TMGR を開く",
  "判断 ({n})",
  "ショートカット…",
  "ログイン時に起動",
  "アップデートを確認…",
  "TMGR を終了",
  "クイック追加",
  "判断 · Doom Solving",
  "ペアリング済みの電話",
  "エージェントのターミナル",
  "ローカルプロフィールの連携を解除",
  "戻る",
  "進む",
];

const KO: Table = [
  "타이머 중지",
  "전환",
  "방해 금지",
  "끄기",
  "1시간 동안",
  "3시간 동안",
  "내일까지",
  "TMGR 열기",
  "결정 ({n})",
  "단축키…",
  "로그인 시 실행",
  "업데이트 확인…",
  "TMGR 종료",
  "빠른 추가",
  "결정 · Doom Solving",
  "페어링된 휴대폰",
  "에이전트 터미널",
  "로컬 프로필 연결 해제",
  "뒤로",
  "앞으로",
];

const PL: Table = [
  "Zatrzymaj timer",
  "Przełącz na",
  "Nie przeszkadzać",
  "Wyłączone",
  "Na 1 godzinę",
  "Na 3 godziny",
  "Do jutra",
  "Otwórz TMGR",
  "Decyzje ({n})",
  "Skróty…",
  "Uruchamiaj przy logowaniu",
  "Sprawdź aktualizacje…",
  "Zakończ TMGR",
  "Szybkie dodawanie",
  "Decyzje · Doom Solving",
  "Sparowane telefony",
  "Terminal agenta",
  "Odłącz profil lokalny",
  "Wstecz",
  "Dalej",
];

fn table(locale: Locale) -> &'static Table {
  match locale {
    Locale::En => &EN,
    Locale::Ru => &RU,
    Locale::Es => &ES,
    Locale::ZhCn => &ZH_CN,
    Locale::PtBr => &PT_BR,
    Locale::De => &DE,
    Locale::Fr => &FR,
    Locale::Ja => &JA,
    Locale::Ko => &KO,
    Locale::Pl => &PL,
  }
}

static CURRENT: Mutex<Locale> = Mutex::new(Locale::En);

pub fn current() -> Locale {
  *CURRENT.lock().unwrap()
}

pub fn tr_in(locale: Locale, key: Key) -> &'static str {
  table(locale)[key as usize]
}

pub fn tr(key: Key) -> &'static str {
  tr_in(current(), key)
}

pub fn decisions_label(count: usize) -> String {
  tr(Key::Decisions).replace("{n}", &count.to_string())
}

fn apply_to_windows<R: Runtime>(app: &AppHandle<R>) {
  let titles = [
    (crate::quick_add::LABEL, Key::QuickAdd),
    #[cfg(unix)]
    (crate::decisions::LABEL, Key::DecisionsWindow),
    #[cfg(unix)]
    (crate::decisions::phone::PHONES_LABEL, Key::PairedPhones),
  ];
  for (label, key) in titles {
    if let Some(window) = app.get_webview_window(label) {
      let _ = window.set_title(tr(key));
    }
  }
}

#[tauri::command]
pub fn native_locale_set<R: Runtime>(app: AppHandle<R>, locale: String) {
  let next = normalize(&locale);
  {
    let mut current = CURRENT.lock().unwrap();
    if *current == next {
      return;
    }
    *current = next;
  }
  crate::tray::refresh_current(&app);
  #[cfg(target_os = "macos")]
  match crate::app_menu::build(&app) {
    Ok(menu) => {
      let _ = app.set_menu(menu);
    }
    Err(err) => log::error!("[i18n] app menu rebuild failed: {err}"),
  }
  apply_to_windows(&app);
}

#[cfg(test)]
mod tests {
  use super::*;

  const ALL_KEYS: [Key; KEY_COUNT] = [
    Key::StopTimer,
    Key::SwitchTo,
    Key::DoNotDisturb,
    Key::DndOff,
    Key::Dnd1h,
    Key::Dnd3h,
    Key::DndTomorrow,
    Key::OpenApp,
    Key::Decisions,
    Key::Shortcuts,
    Key::LaunchAtLogin,
    Key::CheckForUpdates,
    Key::Quit,
    Key::QuickAdd,
    Key::DecisionsWindow,
    Key::PairedPhones,
    Key::AgentTerminal,
    Key::UnlinkProfile,
    Key::Back,
    Key::Forward,
  ];

  #[test]
  fn key_order_matches_the_tables() {
    for (index, key) in ALL_KEYS.iter().enumerate() {
      assert_eq!(*key as usize, index);
    }
  }

  #[test]
  fn every_locale_has_every_key_with_the_english_placeholders() {
    for locale in LOCALES {
      for key in ALL_KEYS {
        let text = tr_in(locale, key);
        assert!(!text.trim().is_empty(), "{} {key:?} is empty", locale.tag());
        assert_eq!(
          text.contains("{n}"),
          tr_in(Locale::En, key).contains("{n}"),
          "{} {key:?} placeholder mismatch",
          locale.tag()
        );
      }
    }
  }

  #[test]
  fn the_brand_stays_in_every_locale() {
    for locale in LOCALES {
      assert!(tr_in(locale, Key::DecisionsWindow).ends_with(" · Doom Solving"), "{}", locale.tag());
      assert!(tr_in(locale, Key::OpenApp).contains("TMGR"), "{}", locale.tag());
      assert!(tr_in(locale, Key::Quit).contains("TMGR"), "{}", locale.tag());
    }
  }

  #[test]
  fn english_matches_the_previous_literals() {
    let expected = [
      "Stop timer",
      "Switch to",
      "Do Not Disturb",
      "Off",
      "For 1 hour",
      "For 3 hours",
      "Until tomorrow",
      "Open TMGR",
      "Decisions ({n})",
      "Shortcuts…",
      "Launch at Login",
      "Check for Updates…",
      "Quit TMGR",
      "Quick add",
      "Decisions · Doom Solving",
      "Paired phones",
      "Agent terminal",
      "Unlink local profile",
      "Back",
      "Forward",
    ];
    for (key, text) in ALL_KEYS.iter().zip(expected) {
      assert_eq!(tr_in(Locale::En, *key), text);
    }
  }

  #[test]
  fn normalize_mirrors_the_frontend_rules() {
    assert_eq!(normalize("ru"), Locale::Ru);
    assert_eq!(normalize("ru-RU"), Locale::Ru);
    assert_eq!(normalize("pt"), Locale::PtBr);
    assert_eq!(normalize("pt_PT"), Locale::PtBr);
    assert_eq!(normalize("zh"), Locale::ZhCn);
    assert_eq!(normalize("zh-CN"), Locale::ZhCn);
    assert_eq!(normalize("zh-Hans"), Locale::ZhCn);
    assert_eq!(normalize("zh-TW"), Locale::En);
    assert_eq!(normalize("zh-Hant"), Locale::En);
    assert_eq!(normalize(" DE "), Locale::De);
    assert_eq!(normalize("xx"), Locale::En);
    assert_eq!(normalize(""), Locale::En);
  }

  #[test]
  fn decisions_label_formats_the_count() {
    assert_eq!(tr_in(Locale::Ru, Key::Decisions).replace("{n}", "3"), "Решения (3)");
  }
}
