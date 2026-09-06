//! A successor acknowledges its visible window before the previous process exits.
use std::{
    io::Write,
    net::{SocketAddr, TcpStream},
    process::Stdio,
    time::Duration,
};
use tauri::Manager;
use tokio::io::AsyncReadExt;

const PORT_ENV: &str = "JARVIS_RELAUNCH_PORT";
const TOKEN_ENV: &str = "JARVIS_RELAUNCH_TOKEN";

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn signal_ready(app: &tauri::AppHandle) {
    let (Ok(port), Ok(token)) = (std::env::var(PORT_ENV), std::env::var(TOKEN_ENV)) else {
        return;
    };
    let Ok(port) = port.parse::<u16>() else {
        return;
    };
    if port == 0 || !valid_token(&token) {
        return;
    }
    // Don't propagate restart credentials to terminals or future app launches.
    std::env::remove_var(PORT_ENV);
    std::env::remove_var(TOKEN_ENV);
    let app = app.clone();
    std::thread::spawn(move || {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        if let Ok(mut socket) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) {
            let _ = socket.set_write_timeout(Some(Duration::from_secs(2)));
            let _ =
                socket.write_all(format!("{token}\n{}\n", app.package_info().version).as_bytes());
            let _ = socket.flush();
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focus();
        }
    });
}

async fn acknowledge(
    listener: &tokio::net::TcpListener,
    token: &str,
    expected_version: &str,
    timeout: Duration,
) -> Result<(), String> {
    let expected = format!("{token}\n{expected_version}\n");
    let receive = async {
        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|_| "Falha ao confirmar a nova janela.")?;
            let mut message = Vec::new();
            let read = tokio::time::timeout(
                Duration::from_secs(2),
                stream.take(256).read_to_end(&mut message),
            )
            .await;
            if matches!(read, Ok(Ok(_))) && message == expected.as_bytes() {
                return Ok(());
            }
        }
    };
    tokio::time::timeout(timeout, receive).await
        .map_err(|_| "A nova janela não confirmou a abertura. O Jarvis atual continuará aberto; tente reabrir novamente.".to_string())?
}

pub(crate) async fn launch_updated(app: &tauri::AppHandle, version: &str) -> Result<(), String> {
    let binary = tauri::process::current_binary(&app.env())
        .map_err(|_| "Não foi possível localizar o Jarvis instalado.")?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "Não foi possível preparar a reabertura do Jarvis.")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Não foi possível preparar a reabertura.")?
        .port();
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| "Não foi possível preparar a reabertura.")?;
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut child = tokio::process::Command::new(binary)
        .env(PORT_ENV, port.to_string()).env(TOKEN_ENV, &token)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|_| "A atualização foi instalada, mas o Jarvis não conseguiu reabrir. Tente reabrir novamente.")?;
    let result = tokio::select! {
        result = acknowledge(&listener, &token, version, Duration::from_secs(30)) => result,
        _ = child.wait() => Err("A nova instância encerrou antes de abrir. O Jarvis atual continuará aberto; tente novamente.".into()),
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn reopens_only_after_expected_token_and_installed_version_are_acknowledged() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let token = "a".repeat(64);
        let expected = token.clone();
        let sender = std::thread::spawn(move || {
            for payload in [
                "wrong\n0.8.1\n".to_owned(),
                format!("{expected}\n0.8.0\n"),
                format!("{expected}\n0.8.1\n"),
            ] {
                let mut stream = TcpStream::connect(address).unwrap();
                stream.write_all(payload.as_bytes()).unwrap();
            }
        });
        acknowledge(&listener, &token, "0.8.1", Duration::from_secs(5))
            .await
            .unwrap();
        sender.join().unwrap();
        assert!(!valid_token("../../file"));
        assert!(valid_token(&token));
    }
    #[tokio::test]
    async fn missing_window_acknowledgment_is_a_recoverable_error() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let error = acknowledge(
            &listener,
            &"a".repeat(64),
            "0.8.1",
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(error.contains("continuará aberto"));
    }
}
