use tmgr_usage::limits::LimitWindow;
use tmgr_usage::link::{attribute, LinkReport};

#[test]
fn link_stub_returns_empty() {
    assert_eq!(attribute(&[], &[]), LinkReport::default());
}

#[test]
fn limit_window_keys_round_trip() {
    for w in [
        LimitWindow::Session,
        LimitWindow::Weekly,
        LimitWindow::Model("fable".into()),
    ] {
        assert_eq!(LimitWindow::from_key(&w.as_key()), Some(w));
    }
    assert_eq!(LimitWindow::from_key("nope"), None);
}
