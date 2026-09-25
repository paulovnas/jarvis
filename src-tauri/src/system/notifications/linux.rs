//! Freedesktop notifications must keep their D-Bus sender alive. Cinnamon removes
//! an application's notification source when that sender disconnects.
use std::{collections::HashMap, time::Duration};
use tokio::sync::Mutex;
use zbus::{zvariant::Value, Connection};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const DEADLINE: Duration = Duration::from_secs(5);
static SESSION: Mutex<Option<Connection>> = Mutex::const_new(None);

async fn connection(session: &mut Option<Connection>) -> Result<Connection, String> {
    if let Some(connection) = session
        .as_ref()
        .filter(|connection| !connection.is_closed())
    {
        return Ok(connection.clone());
    }
    let connection = tokio::time::timeout(DEADLINE, Connection::session())
        .await
        .map_err(|_| "A sessão D-Bus do Linux não respondeu. Reinicie o Jarvis na sua sessão gráfica.".to_string())?
        .map_err(|_| "Não foi possível acessar a sessão D-Bus. Abra o Jarvis pelo ambiente gráfico, sem sudo, e tente novamente.".to_string())?;
    *session = Some(connection.clone());
    Ok(connection)
}

pub(crate) async fn authorize() -> Result<(), String> {
    // Linux has no permission prompt. Check the actual desktop service instead
    // of reporting success merely because the user enabled the preference.
    let connection = connection(&mut *SESSION.lock().await).await?;
    check_service(&connection).await
}

async fn check_service(connection: &Connection) -> Result<(), String> {
    let reply = tokio::time::timeout(
        DEADLINE,
        connection.call_method(Some(SERVICE), PATH, Some(SERVICE), "GetServerInformation", &()),
    )
    .await
    .map_err(|_| "O serviço de notificações do Linux não respondeu. Confira se as notificações estão ativas no ambiente gráfico.".to_string())?
    .map_err(|_| "O serviço de notificações não está disponível nesta sessão Linux. Confira as configurações de notificações do ambiente gráfico e tente novamente.".to_string())?;
    reply
        .body()
        .deserialize::<(String, String, String, String)>()
        .map(|_| ())
        .map_err(|_| {
            "O serviço de notificações do Linux retornou uma resposta inválida.".to_string()
        })
}

pub(crate) async fn show(title: &str, body: &str) -> Result<(), String> {
    deliver(&mut *SESSION.lock().await, title, body).await
}

async fn deliver(session: &mut Option<Connection>, title: &str, body: &str) -> Result<(), String> {
    let connection = connection(session).await?;
    // Tauri installs Jarvis.desktop and the jarvis icon in DEB/AppImage bundles.
    let hints = HashMap::from([
        ("desktop-entry", Value::from("Jarvis")),
        ("urgency", Value::from(1_u8)),
    ]);
    // Notification bodies accept markup on Cinnamon/GNOME; project names do not.
    let body = body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let reply = tokio::time::timeout(
        DEADLINE,
        connection.call_method(
            Some(SERVICE), PATH, Some(SERVICE), "Notify",
            &("Jarvis", 0_u32, "jarvis", title, body, Vec::<String>::new(), hints, -1_i32),
        ),
    )
    .await
    .map_err(|_| "O Linux não confirmou o envio da notificação. Confira o serviço de notificações antes de testar novamente.".to_string())?
    .map_err(|_| "O Linux recusou a notificação. Confira as notificações do Jarvis e o modo Não incomodar no ambiente gráfico.".to_string())?;
    reply
        .body()
        .deserialize::<u32>()
        .map(|_| ())
        .map_err(|_| "O Linux retornou uma confirmação de notificação inválida.".to_string())
}

#[cfg(test)]
#[path = "linux/tests.rs"]
mod tests;
