use super::*;
use std::sync::{Arc, Mutex as StdMutex};
use zbus::{connection::Builder, zvariant::OwnedValue};

#[derive(Debug)]
struct Notice {
    app: String,
    replaces: u32,
    icon: String,
    title: String,
    body: String,
    desktop_entry: String,
    urgency: u8,
    timeout: i32,
}
struct Desktop {
    received: Arc<StdMutex<Vec<Notice>>>,
    reject: bool,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Desktop {
    fn get_server_information(&self) -> (String, String, String, String) {
        ("Fixture".into(), "Jarvis".into(), "1".into(), "1.2".into())
    }

    #[allow(clippy::too_many_arguments)] // The freedesktop Notify wire contract has eight arguments.
    fn notify(
        &self,
        app: String,
        replaces: u32,
        icon: String,
        title: String,
        body: String,
        actions: Vec<String>,
        mut hints: HashMap<String, OwnedValue>,
        timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        assert!(actions.is_empty());
        self.received.lock().unwrap().push(Notice {
            app,
            replaces,
            icon,
            title,
            body,
            timeout,
            desktop_entry: String::try_from(hints.remove("desktop-entry").unwrap()).unwrap(),
            urgency: u8::try_from(hints.remove("urgency").unwrap()).unwrap(),
        });
        if self.reject {
            Err(zbus::fdo::Error::Failed("fixture rejection".into()))
        } else {
            Ok(self.received.lock().unwrap().len() as u32)
        }
    }
}

async fn desktop(reject: bool) -> (Connection, Option<Connection>, Arc<StdMutex<Vec<Notice>>>) {
    // Private peer-to-peer bus: no user session, desktop service or visible banner.
    let (client, server) = tokio::net::UnixStream::pair().unwrap();
    let notices = Arc::new(StdMutex::new(Vec::new()));
    let fixture = Desktop {
        received: notices.clone(),
        reject,
    };
    let server = Builder::unix_stream(server)
        .server(zbus::Guid::generate())
        .unwrap()
        .p2p()
        .serve_at(PATH, fixture)
        .unwrap()
        .build();
    let client = Builder::unix_stream(client).p2p().build();
    let (server, client) = tokio::try_join!(server, client).unwrap();
    (server, Some(client), notices)
}

#[tokio::test]
async fn delivery_retains_the_sender_and_preserves_distinct_notifications() {
    let (desktop, mut session, notices) = desktop(false).await;
    check_service(session.as_ref().unwrap()).await.unwrap();
    deliver(&mut session, "Primeiro aviso", "Projeto <A> & B")
        .await
        .unwrap();
    tokio::task::yield_now().await;
    assert!(
        !desktop.is_closed(),
        "Cinnamon must not see the sender disappear after Notify"
    );
    deliver(&mut session, "Segundo aviso", "Outra conversa")
        .await
        .unwrap();
    assert!(!desktop.is_closed());
    let notices = notices.lock().unwrap();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[0].body, "Projeto &lt;A&gt; &amp; B");
    assert_eq!(notices[1].title, "Segundo aviso");
    for notice in notices.iter() {
        assert_eq!(notice.app, "Jarvis");
        assert_eq!(notice.desktop_entry, "Jarvis");
        assert_eq!(notice.icon, "jarvis");
        assert_eq!(notice.replaces, 0);
        assert_eq!(notice.urgency, 1);
        assert_eq!(notice.timeout, -1);
    }
}

#[tokio::test]
async fn desktop_rejection_is_actionable_and_does_not_replay_the_notification() {
    let (_desktop, mut session, notices) = desktop(true).await;
    let error = deliver(&mut session, "Teste", "Aviso").await.unwrap_err();
    assert!(error.contains("Linux recusou"));
    assert!(error.contains("Não incomodar"));
    assert_eq!(notices.lock().unwrap().len(), 1);
}
