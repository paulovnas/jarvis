//! Advisory local TCP availability checks. Never connect to or stop the owner.
use super::{invalid, AgentError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io::{self, ErrorKind}, net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}};
use tokio::net::TcpSocket;

#[derive(Serialize)]
pub(super) struct Availability {
    port: u16,
    pub available: bool,
}

fn probe(address: SocketAddr) -> io::Result<()> {
    let socket = if address.is_ipv4() { TcpSocket::new_v4()? } else { TcpSocket::new_v6()? };
    // std::net::TcpListener enables SO_REUSEADDR on Unix. On macOS that can
    // allow a wildcard bind beside a specific-address listener: a false free
    // result. Probe without reuse and without ever entering the listen state.
    socket.set_reuseaddr(false)?;
    socket.bind(address)
}

pub(super) fn check(port: u16) -> Result<Availability, AgentError> {
    if port == 0 { return Err(invalid("Informe uma porta TCP entre 1 e 65535.")); }
    // Probe both families, one at a time: a dual-stack IPv6 socket must not
    // collide with our own IPv4 probe. The temporary socket is dropped immediately.
    for ip in [IpAddr::V4(Ipv4Addr::UNSPECIFIED), IpAddr::V6(Ipv6Addr::UNSPECIFIED)] {
        match probe(SocketAddr::new(ip, port)) {
            Ok(()) => {},
            Err(error) if error.kind() == ErrorKind::AddrInUse => return Ok(Availability { port, available: false }),
            Err(error) if ip.is_ipv6() && matches!(error.kind(), ErrorKind::AddrNotAvailable | ErrorKind::Unsupported) => {},
            Err(_) => return Err(invalid("Não foi possível verificar a porta TCP neste computador.")),
        }
    }
    Ok(Availability { port, available: true })
}

pub(super) fn execute(args: &Value) -> Result<Value, AgentError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Args { port: u16 }
    let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid("Informe uma porta TCP entre 1 e 65535."))?;
    serde_json::to_value(check(args.port)?).map_err(|_| AgentError::internal())
}

pub(super) fn definition() -> Value {
    json!({"type":"function","name":"process_check_port","description":"Check whether a local TCP port is available across IPv4 and IPv6 without connecting to, stopping or modifying an existing service. Read the actual port from project configuration first. Call before offering/starting a development server. If occupied, report that no new persistent process was started; do not assume the existing service belongs to this project, switch ports or stop it. Availability is a point-in-time check, not a reservation or a health check. Pass the same port to process_start.","parameters":{"type":"object","properties":{"port":{"type":"integer","minimum":1,"maximum":65535}},"required":["port"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn detects_an_occupied_ipv4_port_without_touching_its_listener() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!check(port).unwrap().available);
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }
    #[test]
    fn available_checks_leave_no_socket_behind() {
        // An ephemeral port may be reused by another test after release.
        for _ in 0..16 {
            let candidate = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = candidate.local_addr().unwrap().port(); drop(candidate);
            if check(port).unwrap().available && probe(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).is_ok() { return; }
        }
        panic!("No temporary port was available after probing");
    }
    #[test]
    fn detects_an_ipv6_listener_even_when_ipv4_is_free() {
        let Ok(listener) = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)) else { return };
        assert!(!check(listener.local_addr().unwrap().port()).unwrap().available);
    }
    #[test]
    fn rejects_invalid_ports_and_remote_scan_arguments() {
        for args in [json!({"port":0}), json!({"port":65536}), json!({"port":-1}), json!({"port":"5173"}), json!({"port":5173,"host":"remote.example"})] {
            assert!(execute(&args).is_err());
        }
    }
}
