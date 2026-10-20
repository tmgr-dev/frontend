use std::path::PathBuf;

pub fn expand(raw: &str) -> Result<PathBuf, String> {
    expand_with(raw, std::env::var_os("HOME").as_deref())
}

pub fn expand_str(raw: &str) -> Result<String, String> {
    expand(raw).map(|p| p.to_string_lossy().into_owned())
}

fn expand_with(raw: &str, home: Option<&std::ffi::OsStr>) -> Result<PathBuf, String> {
    let rest = if raw == "~" || raw == "$HOME" || raw == "${HOME}" {
        Some("")
    } else {
        ["~/", "$HOME/", "${HOME}/"]
            .iter()
            .find_map(|p| raw.strip_prefix(p))
    };
    let Some(rest) = rest else {
        return Ok(PathBuf::from(raw));
    };
    let home = home
        .map(PathBuf::from)
        .filter(|h| h.is_absolute())
        .ok_or_else(|| format!("cannot expand {raw:?}: HOME is not set to an absolute path"))?;
    Ok(if rest.is_empty() {
        home
    } else {
        home.join(rest)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn x(raw: &str) -> Result<PathBuf, String> {
        expand_with(raw, Some(OsStr::new("/h/u")))
    }

    #[test]
    fn expands_a_leading_home_marker_only() {
        assert_eq!(x("~").unwrap(), PathBuf::from("/h/u"));
        assert_eq!(x("~/p/r").unwrap(), PathBuf::from("/h/u/p/r"));
        assert_eq!(x("$HOME/p").unwrap(), PathBuf::from("/h/u/p"));
        assert_eq!(x("${HOME}/p").unwrap(), PathBuf::from("/h/u/p"));
        assert_eq!(x("$HOME").unwrap(), PathBuf::from("/h/u"));
    }

    #[test]
    fn leaves_everything_else_alone() {
        for raw in [
            "/a/~/b", "a/~", "~bob/x", "$HOMEX/y", "$OTHER/z", "x/$HOME", "",
        ] {
            assert_eq!(x(raw).unwrap(), PathBuf::from(raw), "{raw}");
        }
    }

    #[test]
    fn refuses_without_an_absolute_home() {
        assert!(expand_with("~/p", None).is_err());
        assert!(expand_with("~/p", Some(OsStr::new(""))).is_err());
        assert!(expand_with("~/p", Some(OsStr::new("rel"))).is_err());
        assert_eq!(expand_with("/p", None).unwrap(), PathBuf::from("/p"));
    }
}
