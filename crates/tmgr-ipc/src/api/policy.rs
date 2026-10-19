#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ask,
    AcceptEdits,
    Plan,
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    Open,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub mode: Mode,
    pub network: NetworkPolicy,
    pub downgraded_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

pub const NET_NOT_ENFORCED: &str = "network restriction is not enforced on this platform";
pub const NOT_SIGNED: &str = "bypass needs a task approved for agent runs";
pub const NO_SANDBOX: &str = "bypass needs the OS sandbox";

pub fn decide_mode(
    origin: Origin,
    requested: Mode,
    eligible_signed: bool,
    sandbox: bool,
    net_enforced: bool,
) -> Result<Decision, Refusal> {
    if requested != Mode::Bypass {
        return Ok(Decision {
            mode: requested,
            network: NetworkPolicy::Open,
            downgraded_reason: None,
        });
    }
    let blocker = if !eligible_signed {
        Some(NOT_SIGNED)
    } else if !sandbox {
        Some(NO_SANDBOX)
    } else if origin == Origin::Auto && !net_enforced {
        Some(NET_NOT_ENFORCED)
    } else {
        None
    };
    match (blocker, origin) {
        (None, _) => Ok(Decision {
            mode: Mode::Bypass,
            network: if net_enforced {
                NetworkPolicy::Restricted
            } else {
                NetworkPolicy::Open
            },
            downgraded_reason: None,
        }),
        (Some(why), Origin::Manual) => Err(Refusal(why.to_string())),
        (Some(why), Origin::Auto) => Ok(Decision {
            mode: Mode::AcceptEdits,
            network: NetworkPolicy::Open,
            downgraded_reason: Some(why.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [Mode; 4] = [Mode::Ask, Mode::AcceptEdits, Mode::Plan, Mode::Bypass];
    const ORIGINS: [Origin; 2] = [Origin::Manual, Origin::Auto];

    fn expected(
        origin: Origin,
        requested: Mode,
        signed: bool,
        sandbox: bool,
        net: bool,
    ) -> Result<Decision, Refusal> {
        let d = |mode, network, why: Option<&str>| {
            Ok(Decision {
                mode,
                network,
                downgraded_reason: why.map(str::to_string),
            })
        };
        if requested != Mode::Bypass {
            return d(requested, NetworkPolicy::Open, None);
        }
        let fail = |why: &str| match origin {
            Origin::Manual => Err(Refusal(why.to_string())),
            Origin::Auto => d(Mode::AcceptEdits, NetworkPolicy::Open, Some(why)),
        };
        if !signed {
            return fail("bypass needs a task approved for agent runs");
        }
        if !sandbox {
            return fail("bypass needs the OS sandbox");
        }
        if origin == Origin::Auto && !net {
            return fail("network restriction is not enforced on this platform");
        }
        d(
            Mode::Bypass,
            if net {
                NetworkPolicy::Restricted
            } else {
                NetworkPolicy::Open
            },
            None,
        )
    }

    #[test]
    fn the_whole_table() {
        let mut n = 0;
        for origin in ORIGINS {
            for mode in MODES {
                for signed in [false, true] {
                    for sandbox in [false, true] {
                        for net in [false, true] {
                            assert_eq!(
                                decide_mode(origin, mode, signed, sandbox, net),
                                expected(origin, mode, signed, sandbox, net),
                                "{origin:?} {mode:?} signed={signed} sandbox={sandbox} net={net}"
                            );
                            n += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(n, 64);
    }

    #[test]
    fn bypass_only_ever_comes_out_for_signed_sandboxed_requests() {
        for origin in ORIGINS {
            for signed in [false, true] {
                for sandbox in [false, true] {
                    for net in [false, true] {
                        let got = decide_mode(origin, Mode::Bypass, signed, sandbox, net);
                        if let Ok(d) = got {
                            if d.mode == Mode::Bypass {
                                assert!(signed && sandbox);
                                assert_eq!(d.network == NetworkPolicy::Restricted, net);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn other_modes_never_become_bypass() {
        for origin in ORIGINS {
            for mode in [Mode::Ask, Mode::AcceptEdits, Mode::Plan] {
                let d = decide_mode(origin, mode, true, true, true).unwrap();
                assert_eq!(d.mode, mode);
                assert_eq!(d.network, NetworkPolicy::Open);
            }
        }
    }

    #[test]
    fn auto_bypass_without_enforced_network_is_downgraded_with_the_reason() {
        let d = decide_mode(Origin::Auto, Mode::Bypass, true, true, false).unwrap();
        assert_eq!(d.mode, Mode::AcceptEdits);
        assert_eq!(
            d.downgraded_reason.as_deref(),
            Some("network restriction is not enforced on this platform")
        );
    }

    #[test]
    fn manual_bypass_without_approval_is_refused() {
        assert!(decide_mode(Origin::Manual, Mode::Bypass, false, true, true).is_err());
        assert!(decide_mode(Origin::Manual, Mode::Bypass, true, false, true).is_err());
    }
}
