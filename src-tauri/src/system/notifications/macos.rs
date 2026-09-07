use block2::{DynBlock, RcBlock};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{Bool, ProtocolObject},
    AnyThread,
};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSettings,
    UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::{ptr::NonNull, sync::Mutex, time::Duration};
use tokio::sync::oneshot;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "JarvisNotificationDelegate"]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);

fn center() -> Result<Retained<UNUserNotificationCenter>, String> {
    // UserNotifications raises an ObjC exception outside a valid app bundle.
    if NSBundle::mainBundle()
        .bundleIdentifier()
        .is_none_or(|id| id.to_string() != "com.foxtag.jarvis")
    {
        return Err("Teste as notificações pelo aplicativo Jarvis (.app). O executável de desenvolvimento não tem identidade no macOS.".into());
    }
    Ok(UNUserNotificationCenter::currentNotificationCenter())
}

pub(crate) fn setup() {
    if let Ok(center) = center() {
        // Stateless delegate lives for the application lifetime; the OS holds it weakly.
        let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let _ = Retained::into_raw(delegate);
    }
}

fn reply<T>(send: &Mutex<Option<oneshot::Sender<T>>>, value: T) {
    if let Ok(mut send) = send.lock() {
        if let Some(send) = send.take() {
            let _ = send.send(value);
        }
    }
}
async fn receive<T>(receive: oneshot::Receiver<T>, seconds: u64) -> Result<T, String> {
    tokio::time::timeout(Duration::from_secs(seconds), receive).await
        .map_err(|_| "O macOS não respondeu. Confira a permissão de notificações do Jarvis e tente novamente.".to_string())?
        .map_err(|_| "Não foi possível consultar as notificações do macOS.".into())
}
const DENIED: &str = "Notificações bloqueadas. Ative Jarvis em Ajustes do Sistema → Notificações.";

pub(crate) async fn authorize() -> Result<(), String> {
    let (send, received) = oneshot::channel();
    {
        let center = center()?;
        let send = Mutex::new(Some(send));
        let completion = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            reply(
                &send,
                if !error.is_null() {
                    Err("Não foi possível solicitar notificações ao macOS.".into())
                } else if granted.as_bool() {
                    Ok(())
                } else {
                    Err(DENIED.into())
                },
            );
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
            &completion,
        );
    }
    receive(received, 120).await?
}

async fn allowed() -> Result<(), String> {
    let (send, received) = oneshot::channel();
    {
        let center = center()?;
        let send = Mutex::new(Some(send));
        let completion = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            // Apple guarantees a valid settings object for the duration of the callback.
            let status = unsafe { settings.as_ref() }.authorizationStatus();
            reply(
                &send,
                status == UNAuthorizationStatus::Authorized
                    || status == UNAuthorizationStatus::Provisional,
            );
        });
        center.getNotificationSettingsWithCompletionHandler(&completion);
    }
    if receive(received, 10).await? {
        Ok(())
    } else {
        Err(DENIED.into())
    }
}

pub(crate) async fn show(title: &str, body: &str) -> Result<(), String> {
    allowed().await?;
    let (send, received) = oneshot::channel();
    {
        let center = center()?;
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let id =
            crate::library::new_id().map_err(|_| "Não foi possível preparar a notificação.")?;
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&id),
            &content,
            None,
        );
        let send = Mutex::new(Some(send));
        let completion = RcBlock::new(move |error: *mut NSError| {
            reply(
                &send,
                if error.is_null() {
                    Ok(())
                } else {
                    Err("O macOS recusou a notificação. Confira as permissões do Jarvis.".into())
                },
            );
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&completion));
    }
    receive(received, 10).await?
}
