#![cfg(target_os = "macos")]
mod common;

use common::*;
use std::net::TcpListener;
use tmgr_runner::sandbox::Network;

const TCP: &str = r#"use IO::Socket::INET; my $s = IO::Socket::INET->new(PeerAddr => $ARGV[0], PeerPort => $ARGV[1], Proto => "tcp", Timeout => 3); print $s ? "connected\n" : "failed\n";"#;
const UDP: &str = r#"use IO::Socket::INET; my $s = IO::Socket::INET->new(PeerAddr => "1.1.1.1", PeerPort => 53, Proto => "udp"); print(($s && defined $s->send("x")) ? "sent\n" : "failed\n");"#;
const DNS: &str = r#"my @a = gethostbyname("example.com"); print @a ? "resolved\n" : "failed\n";"#;

fn fx() -> Fx {
    fixture("/usr/bin/true")
}

fn listener() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

fn tcp(f: &Fx, net: Network, host: &str, port: u16) -> String {
    let o = f.run_net(&format!("perl -e '{TCP}' {host} {port}"), net);
    o.stdout.trim().to_string()
}

#[test]
fn restricted_blocks_direct_tcp_udp_and_dns() {
    let f = fx();
    let (_proxy, port) = listener();
    let net = Network::Restricted { proxy_port: port };
    let open = f.run_net(&format!("perl -e '{UDP}'"), Network::Open);
    assert_eq!(open.stdout.trim(), "sent", "{}", open.stderr);
    assert_eq!(tcp(&f, net, "1.1.1.1", 443), "failed");
    let o = f.run_net(&format!("perl -e '{UDP}'"), net);
    assert_eq!(o.stdout.trim(), "failed", "{}", o.stderr);
    let o = f.run_net(&format!("perl -e '{DNS}'"), net);
    assert_eq!(o.stdout.trim(), "failed", "{}", o.stderr);
}

#[test]
fn restricted_allows_only_the_proxy_port_on_localhost() {
    let f = fx();
    let (_proxy, port) = listener();
    let (_other, other_port) = listener();
    let net = Network::Restricted { proxy_port: port };
    assert_eq!(tcp(&f, net, "127.0.0.1", port), "connected");
    assert_eq!(tcp(&f, net, "127.0.0.1", other_port), "failed");
    assert_eq!(tcp(&f, Network::Open, "127.0.0.1", other_port), "connected");
}

#[test]
fn restricted_sets_the_proxy_environment() {
    let f = fx();
    let net = Network::Restricted { proxy_port: 4321 };
    let o = f.run_net(
        "echo $HTTPS_PROXY $HTTP_PROXY $https_proxy $http_proxy $ALL_PROXY $all_proxy; echo \"[$NO_PROXY][$no_proxy]\"",
        net,
    );
    let url = "http://127.0.0.1:4321";
    assert_eq!(
        o.stdout.trim(),
        format!("{url} {url} {url} {url} {url} {url}\n[][]")
    );
}

#[test]
fn restricted_keeps_unix_sockets_in_the_run_home() {
    let f = fx();
    let sock = f.home.join("s.sock");
    std::fs::create_dir_all(&f.home).unwrap();
    let _l = std::os::unix::net::UnixListener::bind(&sock).unwrap();
    let script = format!(
        "perl -e 'use Socket; socket(S, PF_UNIX, SOCK_STREAM, 0); print connect(S, sockaddr_un($ARGV[0])) ? qq(connected\\n) : qq(failed\\n)' {}",
        f.path(&sock)
    );
    let o = f.run_net(&script, Network::Restricted { proxy_port: 4321 });
    assert_eq!(o.stdout.trim(), "connected", "{}", o.stderr);
}

#[test]
fn restricted_cannot_hand_a_url_to_the_system_opener() {
    let f = fx();
    let o = f.run_net(
        "/usr/bin/open https://example.com >/dev/null 2>&1; echo $?",
        Network::Restricted { proxy_port: 4321 },
    );
    assert_ne!(o.stdout.trim(), "0", "{}", o.stderr);
}

#[test]
fn restricted_still_runs_ordinary_tools() {
    let f = fx();
    let net = Network::Restricted { proxy_port: 4321 };
    for script in [
        "git --version",
        "/usr/bin/python3 -c 'print(1)'",
        "perl -e 'print 1'",
        "if command -v node >/dev/null; then node -e 'console.log(1)'; fi",
    ] {
        let o = f.run_net(script, net);
        assert!(o.ok(), "{script}: {}", o.stderr);
    }
}
