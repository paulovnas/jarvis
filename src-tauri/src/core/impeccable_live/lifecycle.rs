//! Shutdown uses the authenticated running helper, independent of its app bundle.
use std::{
    io::{ErrorKind, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use super::{error, verify_helper_identity, CoreError};

const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
const IO_TIMEOUT: Duration = Duration::from_secs(1);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const MAX_HEADERS: usize = 8 * 1024;

pub(super) fn stop(identity: &(PathBuf, u16, String)) -> Result<(), CoreError> {
    stop_with_timeout(identity, STOP_TIMEOUT)
}

fn stop_with_timeout(
    identity: &(PathBuf, u16, String),
    timeout: Duration,
) -> Result<(), CoreError> {
    if !verify_helper_identity(identity)? {
        // An absent owned record never authorizes discovery on a reused port.
        return Ok(());
    }
    let deadline = Instant::now() + timeout;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, identity.1));
    let stream =
        match TcpStream::connect_timeout(&address, remaining(deadline)?.min(CONNECT_TIMEOUT)) {
            Ok(stream) => stream,
            Err(cause) if cause.kind() == ErrorKind::ConnectionRefused => return Ok(()),
            Err(_) => {
                return Err(error(
                    "Não foi possível conectar ao helper Live para encerrá-lo.",
                ))
            }
        };
    if !request_stop(stream, identity, deadline)? {
        return Ok(());
    }
    wait_closed(identity, deadline, |timeout| {
        TcpStream::connect_timeout(&address, timeout).map(drop)
    })
}

fn wait_closed(
    identity: &(PathBuf, u16, String),
    deadline: Instant,
    mut probe: impl FnMut(Duration) -> std::io::Result<()>,
) -> Result<(), CoreError> {
    // Upstream removes server.json before it closes its listener. Neither the
    // acknowledgement nor a missing file alone proves that shutdown finished.
    loop {
        verify_helper_identity(identity)?;
        match probe(remaining(deadline)?.min(CONNECT_TIMEOUT)) {
            Ok(()) => (),
            Err(cause) if cause.kind() == ErrorKind::ConnectionRefused => return Ok(()),
            Err(cause)
                if matches!(
                    cause.kind(),
                    ErrorKind::ConnectionReset
                        | ErrorKind::ConnectionAborted
                        | ErrorKind::Interrupted
                        | ErrorKind::TimedOut
                        | ErrorKind::WouldBlock
                ) => {}
            Err(_) => {
                return Err(error(
                    "Não foi possível verificar o encerramento do helper Live.",
                ))
            }
        }
        thread::sleep(remaining(deadline)?.min(POLL_INTERVAL));
    }
}

fn request_stop(
    mut stream: TcpStream,
    identity: &(PathBuf, u16, String),
    deadline: Instant,
) -> Result<bool, CoreError> {
    if !verify_helper_identity(identity)? {
        return Ok(false);
    }
    let token = url::form_urlencoded::byte_serialize(identity.2.as_bytes()).collect::<String>();
    let request = format!(
        "GET /stop?token={token} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        identity.1
    );
    stream.set_write_timeout(Some(remaining(deadline)?.min(IO_TIMEOUT)))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|_| error("Não foi possível solicitar o encerramento do helper Live."))?;
    let mut headers = Vec::new();
    let mut chunk = [0; 1024];
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?.min(IO_TIMEOUT)))?;
        let count = stream
            .read(&mut chunk)
            .map_err(|_| error("O helper Live não confirmou a solicitação de encerramento."))?;
        if count == 0 || headers.len() + count > MAX_HEADERS {
            return Err(error(
                "O helper Live retornou uma confirmação de encerramento inválida.",
            ));
        }
        headers.extend_from_slice(&chunk[..count]);
        if headers.windows(4).any(|part| part == b"\r\n\r\n") {
            break;
        }
    }
    let status = headers
        .split(|byte| *byte == b'\n')
        .next()
        .and_then(|line| std::str::from_utf8(line).ok())
        .map(|line| {
            let mut fields = line.split_whitespace();
            matches!(fields.next(), Some("HTTP/1.0" | "HTTP/1.1")) && fields.next() == Some("200")
        });
    if status != Some(true) {
        return Err(error("O helper Live recusou a solicitação de encerramento. A identidade da sessão foi preservada."));
    }
    Ok(true)
}

fn remaining(deadline: Instant) -> Result<Duration, CoreError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| error("O helper Live não encerrou no prazo. Tente retomar o modo Live."))
}

#[cfg(test)]
mod tests {
    use std::{fs, net::TcpListener};

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn fixture(token: &str) -> (TempDir, (PathBuf, u16, String), TcpListener) {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let identity = (
            directory.path().join("server.json"),
            listener.local_addr().unwrap().port(),
            token.to_owned(),
        );
        fs::write(
            &identity.0,
            json!({"pid":std::process::id(),"port":identity.1,"token":identity.2}).to_string(),
        )
        .unwrap();
        (directory, identity, listener)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        loop {
            let count = stream.read(&mut chunk).unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&chunk[..count]);
            assert!(request.len() < MAX_HEADERS);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                return String::from_utf8(request).unwrap();
            }
        }
    }

    #[test]
    fn stop_authenticates_then_waits_until_the_listener_has_closed() {
        let (_directory, identity, listener) = fixture("private+token&value");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nstopping")
                .unwrap();
            thread::sleep(Duration::from_millis(60));
            drop(listener);
            request
        });
        stop(&identity).unwrap();
        assert!(worker
            .join()
            .unwrap()
            .starts_with("GET /stop?token=private%2Btoken%26value HTTP/1.1\r\n"));
        assert!(
            identity.0.is_file(),
            "host must not unlink the server record"
        );
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, identity.1)).is_err());
    }

    #[test]
    fn transient_shutdown_probes_wait_for_connection_refusal() {
        let (_directory, identity, _listener) = fixture("owned");
        let mut outcomes = std::collections::VecDeque::from([
            Err(ErrorKind::ConnectionReset),
            Err(ErrorKind::ConnectionAborted),
            Err(ErrorKind::Interrupted),
            Err(ErrorKind::TimedOut),
            Err(ErrorKind::WouldBlock),
            Ok(()),
            Err(ErrorKind::ConnectionRefused),
        ]);
        wait_closed(&identity, Instant::now() + STOP_TIMEOUT, |_| {
            outcomes.pop_front().unwrap().map_err(std::io::Error::from)
        })
        .unwrap();
        assert!(outcomes.is_empty(), "only refusal confirms closure");
        assert!(identity.0.is_file());
    }

    #[test]
    fn recurring_transient_shutdown_errors_still_expire() {
        let (_directory, identity, _listener) = fixture("owned");
        let result = wait_closed(
            &identity,
            Instant::now() + Duration::from_millis(100),
            |_| Err(ErrorKind::ConnectionReset.into()),
        );
        assert!(result.unwrap_err().message.contains("prazo"));
        assert!(identity.0.is_file());
    }

    #[test]
    fn permanent_shutdown_probe_errors_preserve_the_owned_record() {
        let (_directory, identity, _listener) = fixture("owned");
        let mut attempts = 0;
        let result = wait_closed(&identity, Instant::now() + STOP_TIMEOUT, |_| {
            attempts += 1;
            Err(ErrorKind::PermissionDenied.into())
        });
        assert!(result.unwrap_err().message.contains("verificar"));
        assert_eq!(attempts, 1);
        assert!(identity.0.is_file());
    }

    #[test]
    fn missing_owned_record_does_not_contact_an_external_listener() {
        let (_directory, identity, listener) = fixture("owned");
        fs::remove_file(&identity.0).unwrap();
        stop(&identity).unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
    }

    #[test]
    fn substituted_identity_does_not_contact_or_unlink_an_external_helper() {
        let (_directory, identity, listener) = fixture("owned");
        let replacement = json!({"port":identity.1,"token":"external"}).to_string();
        fs::write(&identity.0, &replacement).unwrap();
        assert!(stop(&identity).is_err());
        listener.set_nonblocking(true).unwrap();
        assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
        assert_eq!(fs::read_to_string(&identity.0).unwrap(), replacement);
    }

    #[test]
    fn identity_is_revalidated_after_connecting_and_before_sending_stop() {
        let (_directory, identity, listener) = fixture("owned");
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, identity.1)).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        fs::write(
            &identity.0,
            json!({"port":identity.1,"token":"external"}).to_string(),
        )
        .unwrap();
        assert!(request_stop(stream, &identity, Instant::now() + STOP_TIMEOUT).is_err());
        peer.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
        assert!(identity.0.is_file());
    }

    #[test]
    fn an_authentication_rejection_preserves_the_owned_record() {
        let (_directory, identity, listener) = fixture("owned");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        assert!(stop(&identity).unwrap_err().message.contains("recusou"));
        worker.join().unwrap();
        assert!(identity.0.is_file());
    }

    #[test]
    fn a_stop_acknowledgement_does_not_hide_a_still_running_listener() {
        let (_directory, identity, listener) = fixture("owned");
        let peer_listener = listener.try_clone().unwrap();
        let worker = thread::spawn(move || {
            let (mut stream, _) = peer_listener.accept().unwrap();
            read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nstopping")
                .unwrap();
        });
        assert!(stop_with_timeout(&identity, Duration::from_millis(200))
            .unwrap_err()
            .message
            .contains("prazo"));
        worker.join().unwrap();
        assert!(identity.0.is_file());
        assert!(TcpStream::connect(listener.local_addr().unwrap()).is_ok());
    }

    #[test]
    fn a_nonresponding_helper_is_bounded_and_preserves_its_record() {
        let (_directory, identity, listener) = fixture("owned");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            thread::sleep(Duration::from_millis(250));
        });
        assert!(stop_with_timeout(&identity, Duration::from_millis(100)).is_err());
        worker.join().unwrap();
        assert!(identity.0.is_file());
    }

    #[test]
    fn an_already_closed_helper_does_not_require_its_previous_executable() {
        let (_directory, identity, listener) = fixture("owned");
        drop(listener);
        stop(&identity).unwrap();
        assert!(identity.0.is_file());
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "uses the bundled native engine in a private temporary project"]
    fn native_helper_stops_after_its_app_bundle_is_moved() {
        use std::process::{Child, Command, Stdio};

        struct Helper(Child);
        impl Drop for Helper {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/impeccable");
        let engine_name = if cfg!(windows) {
            "impeccable.exe"
        } else {
            "impeccable"
        };
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("OldJarvis.app");
        fs::create_dir_all(bundle.join("bin")).unwrap();
        let executable = bundle.join("bin").join(engine_name);
        fs::copy(resources.join("bin").join(engine_name), &executable).unwrap();
        let skill = bundle.join(".agents/skills/impeccable");
        fs::create_dir_all(&skill).unwrap();
        fs::copy(
            resources.join(".agents/skills/impeccable/SKILL.md"),
            skill.join("SKILL.md"),
        )
        .unwrap();
        let project = directory.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("package.json"),
            r#"{"name":"live-stop-fixture"}"#,
        )
        .unwrap();
        let mut helper = Helper(
            Command::new(&executable)
                .args(["live-server", "--port=0"])
                .current_dir(&project)
                .env("IMPECCABLE_HOME", directory.path().join("home"))
                .env("IMPECCABLE_SKILL_DIR", &skill)
                .env_remove("IMPECCABLE_DESIGN_PATH")
                .env_remove("IMPECCABLE_DESIGN_ROOT")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let record = project.join(".impeccable/live/server.json");
        let deadline = Instant::now() + STOP_TIMEOUT;
        while !record.is_file() {
            assert!(
                helper.0.try_wait().unwrap().is_none(),
                "helper exited before startup"
            );
            assert!(
                Instant::now() < deadline,
                "helper did not publish its identity"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let info: serde_json::Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        let identity = (
            record,
            u16::try_from(info["port"].as_u64().unwrap()).unwrap(),
            info["token"].as_str().unwrap().to_owned(),
        );
        fs::rename(&bundle, directory.path().join("TrashedJarvis.app")).unwrap();
        assert!(!executable.exists());
        stop(&identity).unwrap();
        let deadline = Instant::now() + STOP_TIMEOUT;
        let status = loop {
            if let Some(status) = helper.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "helper retained its process after shutdown"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success());
        assert!(!identity.0.exists(), "helper should clean its own record");
    }
}
