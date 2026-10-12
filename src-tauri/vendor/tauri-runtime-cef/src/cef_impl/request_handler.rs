// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::{
  borrow::Cow,
  io::{Cursor, Read},
  sync::{mpsc, Arc, Mutex, OnceLock},
};

use cef::{rc::*, *};
use dioxus_debug_cell::RefCell;
use html5ever::{interface::QualName, namespace_url, ns, LocalName};
use http::{
  header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, ORIGIN},
  HeaderMap, HeaderName, HeaderValue,
};
use kuchiki::{Attribute, ExpandedName, NodeRef};
use tauri_runtime::{
  webview::{NavigationHandler, UriSchemeProtocolHandler},
  window::WindowId,
  UserEvent,
};
use tauri_utils::{
  config::{Csp, CspDirectiveSources},
  html::{parse as parse_html, serialize_node},
};
use url::Url;

use crate::{
  cef_impl::client::{
    DragDropEventTarget, DragDropState, WebDragDropResourceRequestHandler,
    WebDragDropResourceRequestHandlerArgs,
  },
  macros::wrap_with_args,
  runtime::RuntimeContext,
  webview::{CefInitScript, INITIAL_LOAD_URL},
};

type HttpResponse = Arc<RefCell<Option<http::Response<Cursor<Vec<u8>>>>>>;
pub(crate) type SchemeRegistry = Arc<
  Mutex<
    std::collections::HashMap<
      (i32, String),
      (
        String,
        Arc<Box<UriSchemeProtocolHandler>>,
        Arc<Vec<CefInitScript>>,
      ),
    >,
  >,
>;

type RequestJob = Box<dyn FnOnce() + Send + 'static>;
type ProtocolResponse = http::Response<Cow<'static, [u8]>>;
type ProtocolResponder = Box<dyn FnOnce(ProtocolResponse) + Send>;

// Dispatch only the protocol handler here. An asynchronous handler retains its
// responder and releases the worker immediately; waiting for that response
// would deadlock the shared pool behind unrelated long-running invocations.
struct RequestExecutor {
  sender: mpsc::SyncSender<RequestJob>,
}

impl RequestExecutor {
  fn new(worker_count: usize, queue_capacity: usize) -> std::io::Result<Self> {
    if worker_count == 0 {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "CEF protocol dispatch requires a worker",
      ));
    }
    let (sender, receiver) = mpsc::sync_channel::<RequestJob>(queue_capacity);
    let receiver = Arc::new(Mutex::new(receiver));
    for index in 0..worker_count {
      let receiver = receiver.clone();
      std::thread::Builder::new()
        .name(format!("tauri-cef-protocol-{index}"))
        .spawn(move || {
          loop {
            let job = {
              let receiver = receiver
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
              receiver.recv()
            };
            let Ok(job) = job else { break };
            // User protocol handlers must not permanently remove a worker.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
              log::error!("CEF protocol dispatch panicked");
            }
          }
        })?;
    }
    Ok(Self { sender })
  }

  fn try_dispatch(&self, job: RequestJob) -> Result<(), mpsc::TrySendError<RequestJob>> {
    self.sender.try_send(job)
  }
}

fn request_executor() -> Option<&'static RequestExecutor> {
  static EXECUTOR: OnceLock<Option<RequestExecutor>> = OnceLock::new();
  EXECUTOR
    .get_or_init(|| match RequestExecutor::new(8, 256) {
      Ok(executor) => Some(executor),
      Err(error) => {
        log::error!("CEF protocol workers could not start: {error}");
        None
      }
    })
    .as_ref()
}

#[derive(Clone)]
struct PendingProtocolResponse(Arc<Mutex<Option<ProtocolResponder>>>);

impl PendingProtocolResponse {
  fn new(responder: ProtocolResponder) -> Self {
    Self(Arc::new(Mutex::new(Some(responder))))
  }

  fn respond(&self, response: ProtocolResponse) {
    let responder = self
      .0
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .take();
    // CEF continuation or an application callback can be reentrant.
    if let Some(responder) = responder {
      responder(response);
    }
  }
}

fn dispatch_protocol(
  executor: Option<&RequestExecutor>,
  handler: Arc<Box<UriSchemeProtocolHandler>>,
  label: String,
  request: http::Request<Vec<u8>>,
  responder: ProtocolResponder,
) {
  if let Some(response) = ipc_preflight_response(&request) {
    responder(response);
    return;
  }
  let pending = PendingProtocolResponse::new(responder);
  let pending_worker = pending.clone();
  let job = Box::new(move || {
    let pending_response = pending_worker.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
      (**handler)(
        &label,
        request,
        Box::new(move |response| pending_response.respond(response)),
      );
    }));
    if result.is_err() {
      log::error!("CEF protocol handler panicked");
      pending_worker.respond(protocol_error_response(
        http::StatusCode::INTERNAL_SERVER_ERROR,
        "\"The local request failed. Try again.\"",
      ));
    }
  });
  if executor.is_none_or(|executor| executor.try_dispatch(job).is_err()) {
    // Reject before the handler runs. A CORS-readable IPC error prevents the
    // frontend's fetch-failure fallback from resubmitting the same invocation.
    pending.respond(protocol_error_response(
      http::StatusCode::SERVICE_UNAVAILABLE,
      "\"The local service is busy. Try again.\"",
    ));
  }
}

fn ipc_preflight_response(request: &http::Request<Vec<u8>>) -> Option<ProtocolResponse> {
  let uri = request.uri();
  let is_ipc = uri.scheme_str() == Some("ipc")
    || (matches!(uri.scheme_str(), Some("http" | "https")) && uri.host() == Some("ipc.localhost"));
  if request.method() != http::Method::OPTIONS || !is_ipc {
    return None;
  }
  // Tauri's IPC OPTIONS handler is a static CORS response. It must remain
  // available under pressure so the POST can receive the structured 503
  // instead of failing preflight and bypassing admission through postMessage.
  let mut response = http::Response::new(Cow::Borrowed(&[][..]));
  let headers = response.headers_mut();
  headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
  headers.insert(
    "access-control-allow-headers",
    HeaderValue::from_static("*"),
  );
  headers.insert(
    "access-control-allow-methods",
    HeaderValue::from_static("POST, OPTIONS"),
  );
  headers.insert(
    "access-control-expose-headers",
    HeaderValue::from_static("Tauri-Response"),
  );
  Some(response)
}

fn protocol_error_response(status: http::StatusCode, message: &'static str) -> ProtocolResponse {
  let mut response = http::Response::new(Cow::Borrowed(message.as_bytes()));
  *response.status_mut() = status;
  let headers = response.headers_mut();
  headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
  headers.insert("tauri-response", HeaderValue::from_static("error"));
  headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
  headers.insert(
    "access-control-expose-headers",
    HeaderValue::from_static("Tauri-Response"),
  );
  headers.insert("retry-after", HeaderValue::from_static("1"));
  response
}

fn csp_inject_initialization_scripts_hashes(
  existing_csp: String,
  initialization_scripts: &[CefInitScript],
  is_main_frame: bool,
) -> String {
  if initialization_scripts.is_empty() {
    return existing_csp;
  }

  let script_hashes: Vec<String> = initialization_scripts
    .iter()
    .filter(|script| script.runs_in_frame(is_main_frame))
    .map(|s| s.hash.clone())
    .collect();

  if script_hashes.is_empty() {
    return existing_csp;
  }

  let mut csp_map: std::collections::HashMap<String, CspDirectiveSources> =
    Csp::Policy(existing_csp.to_string()).into();

  let script_src = csp_map
    .entry("script-src".to_string())
    .or_insert_with(|| CspDirectiveSources::List(vec!["'self'".to_string()]));

  script_src.extend(script_hashes);

  Csp::DirectiveMap(csp_map).to_string()
}

fn inject_scripts_into_html_body(
  body: &[u8],
  initialization_scripts: &[CefInitScript],
  is_main_frame: bool,
) -> Option<Vec<u8>> {
  let Ok(body_str) = std::str::from_utf8(body) else {
    return None;
  };

  let document = parse_html(body_str.to_string());

  let head = if let Ok(ref head_node) = document.select_first("head") {
    head_node.as_node().clone()
  } else {
    let head_node = NodeRef::new_element(
      QualName::new(None, ns!(html), LocalName::from("head")),
      None,
    );
    document.prepend(head_node.clone());
    head_node
  };

  for init_script in initialization_scripts
    .iter()
    .rev()
    .filter(|script| script.runs_in_frame(is_main_frame))
  {
    let script_el = NodeRef::new_element(QualName::new(None, ns!(html), "script".into()), None);
    script_el.append(NodeRef::new_text(init_script.script.as_str()));
    head.prepend(script_el);
  }

  keep_encoding_declaration_first(&document, &head);

  Some(serialize_node(&document))
}

/// Keeps the document's character encoding declaration ahead of the injected scripts.
///
/// Chromium only pre-scans the first 1024 bytes of a document for a `<meta charset>`
/// declaration. The initialization scripts we prepend to `<head>` are much larger than
/// that, so an encoding declaration that used to be in the pre-scan window ends up out
/// of reach and the document is decoded with the fallback encoding (windows-1252)
/// instead. That mojibakes every non-ASCII byte in the page and in the injected scripts,
/// which additionally breaks the CSP hashes we compute over the UTF-8 source, so the
/// browser refuses to execute the affected script.
fn keep_encoding_declaration_first(document: &NodeRef, head: &NodeRef) {
  let existing_declaration = document.select("meta").ok().and_then(|metas| {
    metas.into_iter().find(|meta| {
      let attributes = meta.attributes.borrow();
      attributes.get("charset").is_some()
        || attributes
          .get("http-equiv")
          .map(|value| value.trim().eq_ignore_ascii_case("content-type"))
          .unwrap_or(false)
    })
  });

  match existing_declaration {
    // the document declares its encoding, move that declaration back into the pre-scan window
    Some(declaration) => {
      let node = declaration.as_node().clone();
      node.detach();
      head.prepend(node);
    }
    // no declaration at all: the assets are served as UTF-8 (this handler parses them as
    // such), so make that explicit instead of leaving it to the fallback encoding
    None => head.prepend(NodeRef::new_element(
      QualName::new(None, ns!(html), LocalName::from("meta")),
      [(
        ExpandedName::new(ns!(), LocalName::from("charset")),
        Attribute {
          prefix: None,
          value: "utf-8".into(),
        },
      )],
    )),
  }
}

wrap_with_args! {
  wrap_request_handler => WebRequestHandlerArgs;

  pub struct WebRequestHandler<T: UserEvent> {
    navigation_handler: Option<Arc<NavigationHandler>>,
    frame_event_handler: Option<Arc<crate::FrameEventHandler>>,
    context: RuntimeContext<T>,
    window_id: WindowId,
    webview_id: u32,
    drag_drop_event_target: DragDropEventTarget,
    drag_drop_handler_enabled: bool,
    drag_drop_state: Arc<Mutex<DragDropState>>,
    web_content_process_terminate_handler: Option<Arc<tauri_runtime::webview::OnWebContentProcessTerminateHandler>>,
    certificate_errors: crate::CertificateErrorPolicy,
  }

  impl RequestHandler {
    /// Answers a TLS certificate that does not validate, per
    /// [`CertificateErrorPolicy`](crate::CertificateErrorPolicy).
    ///
    /// Returning 0 hands the error back to Chromium, which shows the SSL interstitial
    /// with its "proceed anyway" link. Cancelling instead means dropping the callback
    /// without continuing it: CEF reads a `false` return as "cancel this request", and
    /// the page gets a network error it cannot click past.
    fn on_certificate_error(
      &self,
      _browser: Option<&mut Browser>,
      cert_error: Errorcode,
      request_url: Option<&CefString>,
      _ssl_info: Option<&mut Sslinfo>,
      _callback: Option<&mut Callback>,
    ) -> ::std::os::raw::c_int {
      match self.certificate_errors {
        crate::CertificateErrorPolicy::ChromeInterstitial => 0,
        crate::CertificateErrorPolicy::Cancel => {
          // The URL can carry credentials and tokens, so only its origin is logged.
          let origin = request_url
            .map(ToString::to_string)
            .and_then(|url| url::Url::parse(&url).ok())
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_else(|| "an unknown origin".to_string());
          log::warn!(
            "cancelling a request to {origin}: its TLS certificate did not validate \
             (Chromium error {cert_error:?}), and the runtime is set to \
             CertificateErrorPolicy::Cancel"
          );
          1
        }
      }
    }
    fn on_render_process_terminated(
      &self,
      browser: Option<&mut Browser>,
      status: TerminationStatus,
      error_code: ::std::os::raw::c_int,
      error_string: Option<&CefString>,
    ) {
      let mut frame = browser.as_ref().and_then(|browser| browser.main_frame());
      crate::frame::emit_frame_event(&self.frame_event_handler, browser, frame.as_mut(), crate::FrameEventKind::RendererTerminated);
      if let Some(handler) = &self.web_content_process_terminate_handler {
        handler(tauri_runtime::webview::WebContentProcessTermination {
          reason: termination_reason(status),
          error_code: Some(error_code),
          error_string: error_string.map(ToString::to_string),
        });
      }
    }

    fn on_before_browse(
      &self,
      browser: Option<&mut Browser>,
      frame: Option<&mut Frame>,
      request: Option<&mut Request>,
      _user_gesture: ::std::os::raw::c_int,
      _is_redirect: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int {
      let _ = (&self.context, self.window_id, self.webview_id);

      let Some(frame) = frame else {
        return 0;
      };
      let Some(request) = request else {
        return 0;
      };

      let url_str = CefString::from(&request.url()).to_string();

      if url_str == INITIAL_LOAD_URL {
        return 0;
      }

      let Ok(url) = url::Url::parse(&url_str) else {
        return 0;
      };

      // Preserve the portable main-frame policy. Native observers receive only
      // admitted navigations, so a denied navigation cannot strand their barrier.
      if frame.is_main() != 0
        && self.navigation_handler.as_ref().is_some_and(|handler| !handler(&url))
      {
        return 1;
      }
      crate::frame::emit_frame_event(
        &self.frame_event_handler,
        browser,
        Some(frame),
        crate::FrameEventKind::NavigationStarted { url },
      );
      0
    }

    fn resource_request_handler(
      &self,
      _browser: Option<&mut Browser>,
      _frame: Option<&mut Frame>,
      _request: Option<&mut Request>,
      _is_navigation: ::std::os::raw::c_int,
      _is_download: ::std::os::raw::c_int,
      _request_initiator: Option<&CefString>,
      _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
    ) -> Option<ResourceRequestHandler> {
      // The handler only intercepts the drag-drop bridge requests; when the
      // bridge is disabled it would pass every request straight through, so skip
      // building (and cloning the context + state into) a handler that CEF calls
      // for every subresource/fetch/XHR the page makes.
      if !self.drag_drop_handler_enabled {
        return None;
      }

      Some(WebDragDropResourceRequestHandler::build(
        WebDragDropResourceRequestHandlerArgs {
          context: self.context.clone(),
          window_id: self.window_id,
          webview_id: self.webview_id,
          drag_drop_event_target: self.drag_drop_event_target,
          drag_drop_handler_enabled: self.drag_drop_handler_enabled,
          drag_drop_state: self.drag_drop_state.clone(),
        },
      ))
    }
  }
}

wrap_with_args! {
  wrap_resource_handler => WebResourceHandlerArgs;

  pub struct WebResourceHandler {
    webview_label: String,
    handler: Arc<Box<UriSchemeProtocolHandler>>,
    initialization_scripts: Arc<Vec<CefInitScript>>,
    // Whether the document this response is loaded into is the main frame. Scripts flagged
    // `for_main_frame_only` are skipped for subframes - the isolation iframe most notably -
    // matching both the wry runtime and the guard in the document-start script we register
    // over CDP for documents that are not served by a custom protocol.
    is_main_frame: bool,
    // Serialized origin of the main frame that initiated this request, captured
    // browser-side in the scheme handler factory. The renderer can issue an IPC
    // request before its execution context is fully wired to the loader; in
    // that window Chromium tags the request with `Origin: null` even though the
    // document already has a proper origin. We use this to repair the `Origin`
    // header in that case. `None` when the initiator is not the (non-opaque)
    // main frame, so sandboxed/subframe `Origin: null` requests are left as-is.
    initiator_origin: Option<String>,
    // we clone response to send it to the handler thread
    response: HttpResponse,
  }

  impl ResourceHandler {
    fn process_request(
      &self,
      request: Option<&mut Request>,
      callback: Option<&mut Callback>,
    ) -> ::std::os::raw::c_int {
      let Some(request) = request else { return 0 };
      let Some(callback) = callback else { return 0 };

      let url = CefString::from(&request.url()).to_string();
      let url = Url::parse(&url).ok();

      if let Some(url) = url {
        let callback = ThreadSafe(callback.clone());
        let response_store = ThreadSafe(self.response.clone());
        let initialization_scripts = self.initialization_scripts.clone();
        let is_main_frame = self.is_main_frame;
        let responder: ProtocolResponder = Box::new(move |response: http::Response<Cow<'static, [u8]>>| {
          let is_html = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .map(|ct| ct.to_lowercase().starts_with("text/html"))
            .unwrap_or(false);

          let (parts, body) = response.into_parts();
          let body_bytes = body.into_owned();
          let body_bytes = if is_html {
            inject_scripts_into_html_body(&body_bytes, &initialization_scripts, is_main_frame)
              .unwrap_or(body_bytes)
          } else {
            body_bytes
          };

          let mut response = http::Response::from_parts(parts, Cursor::new(body_bytes));

          if let Some(csp) = response.headers_mut().get_mut(CONTENT_SECURITY_POLICY) {
            let csp_string = csp.to_str().unwrap_or_default().to_string();
            let new_csp = csp_inject_initialization_scripts_hashes(
              csp_string,
              &initialization_scripts,
              is_main_frame,
            );
            if let Ok(new_csp) = HeaderValue::from_str(&new_csp) {
              *csp = new_csp;
            }
          }

          response_store.into_owned().borrow_mut().replace(response);

          let callback = callback.into_owned();
          callback.cont();
        });

        let label = self.webview_label.clone();
        let handler = self.handler.clone();

        let data = read_request_body(request);
        let mut headers = get_request_headers(request);

        // The renderer can issue an IPC request before its execution context is
        // fully wired to the loader; in that window Chromium sends the request
        // with `Origin: null` even though the document already has a real
        // origin. Repair it from the initiating main frame's URL, which the
        // browser process tracks reliably. Only done when the renderer sent no
        // origin or a literal `null`, so a correct renderer-sent origin always
        // wins.
        if let Some(initiator_origin) = &self.initiator_origin {
          let origin_missing_or_null = headers
            .get(ORIGIN)
            .map(|value| value.as_bytes() == b"null")
            .unwrap_or(true);
          if origin_missing_or_null && let Ok(value) = HeaderValue::from_str(initiator_origin) {
            headers.insert(ORIGIN, value);
          }
        }

        let method_str = CefString::from(&request.method()).to_string();
        let method = http::Method::from_bytes(method_str.as_bytes()).unwrap_or(http::Method::GET);

        let http_request = http::Request::builder()
            .method(method)
            .uri(url.as_str())
            .body(data);
        match http_request {
          Ok(mut http_request) => {
            *http_request.headers_mut() = headers;
            dispatch_protocol(request_executor(), handler, label, http_request, responder);
          }
          Err(_) => responder(protocol_error_response(
            http::StatusCode::BAD_REQUEST,
            "\"The local request URL is invalid.\"",
          )),
        }
        1
      } else {
        0
      }
    }

    fn read(
      &self,
      data_out: *mut u8,
      bytes_to_read: ::std::os::raw::c_int,
      bytes_read: Option<&mut ::std::os::raw::c_int>,
      _callback: Option<&mut ResourceReadCallback>,
    ) -> ::std::os::raw::c_int {
      let Ok(bytes_to_read) = usize::try_from(bytes_to_read) else {
        return 0;
      };
      let data_out = unsafe { std::slice::from_raw_parts_mut(data_out, bytes_to_read) };
      let count = self
        .response
        .borrow_mut()
        .as_mut()
        .and_then(|response| response.body_mut().read(data_out).ok())
        .unwrap_or(0);
      if let Some(bytes_read) = bytes_read {
        let Ok(count) = count.try_into() else {
          return 0;
        };
        *bytes_read = count;
        if count > 0 {
          return 1;
        }
      }
      0
    }

    fn response_headers(
      &self,
      response: Option<&mut Response>,
      response_length: Option<&mut i64>,
      redirect_url: Option<&mut CefString>,
    ) {
      let (Some(response), Some(response_data)) = (response, &*self.response.borrow()) else {
        return;
      };

      response.set_status(response_data.status().as_u16() as i32);
      let mut content_type = None;

      // Set response headers and remember the MIME type for CEF.
      for (name, value) in response_data.headers() {
        let Ok(value) = value.to_str() else {
          continue;
        };

        response.set_header_by_name(Some(&name.as_str().into()), Some(&value.into()), 0);

        if name == CONTENT_TYPE {
          content_type.replace(value.to_string());
        }
      }

      response.set_header_by_name(Some(&"Cache-Control".into()), Some(&"no-store".into()), 1);

      let mime_type = content_type
        .as_ref()
        .and_then(|t| t.split(';').next())
        .map(str::trim)
        .unwrap_or("text/plain");
      response.set_mime_type(Some(&mime_type.into()));

      if let Some(length) = response_length {
        *length = -1;
      }

      if let Some(redirect_url) = redirect_url {
        let _ = std::mem::take(redirect_url);
      }
    }
  }
}

wrap_scheme_handler_factory! {
  pub struct UriSchemeHandlerFactory {
    registry: SchemeRegistry,
    scheme: String,
  }

  impl SchemeHandlerFactory {
    fn create(
      &self,
      browser: Option<&mut Browser>,
      frame: Option<&mut Frame>,
      _scheme_name: Option<&CefString>,
      _request: Option<&mut Request>,
    ) -> Option<ResourceHandler> {
      let (webview_label, handler, initialization_scripts) = match browser {
        Some(browser) => {
          let id = browser.identifier();

          // get handler from our regsitry based on browser ID and scheme
          self
            .registry
            .lock()
            .unwrap()
            .get(&(id, self.scheme.clone()))
            .cloned()?
        }
        // `browser`/`frame` are null for requests that do not originate from a
        // browser: service worker (main script and update) fetches and
        // CefURLRequest. Returning `None` here would fall through to the
        // network service, where `{scheme}.localhost` resolves to loopback and
        // the connection is refused — which is exactly how service worker
        // registration on a custom-protocol origin used to fail with "An
        // unknown error occurred when fetching the script.".
        //
        // Every registry entry for a given scheme wraps the same app-level
        // `UriSchemeProtocolHandler`, so any live entry can serve the request;
        // only the webview label differs, and asset serving does not depend on
        // it. Initialization scripts are per-document injections, meaningless
        // without a browser, so they are not applied on this path.
        None => {
          let (webview_label, handler, _) = self
            .registry
            .lock()
            .unwrap()
            .iter()
            .find(|((_, scheme), _)| *scheme == self.scheme)
            .map(|(_, entry)| entry.clone())?;
          (webview_label, handler, Arc::new(Vec::new()))
        }
      };

      // A subframe navigation is the only case we can positively identify here, so anything
      // else (a null frame included) keeps the main frame behavior of injecting everything.
      let is_main_frame = frame.as_ref().map(|frame| frame.is_main() == 1).unwrap_or(true);

      // Capture the initiating main frame's origin so `process_request` can
      // repair a racy `Origin: null` header. Restricted to the main frame: it
      // is never an opaque-origin (sandboxed) document in a Tauri webview, so
      // upgrading its origin is safe; subframes are intentionally left alone.
      let initiator_origin = frame
        .filter(|frame| frame.is_main() == 1)
        .map(|frame| CefString::from(&frame.url()).to_string())
        .and_then(|url| Url::parse(&url).ok())
        .map(|url| url.origin().ascii_serialization())
        .filter(|origin| origin != "null");

      Some(WebResourceHandler::build(WebResourceHandlerArgs {
        webview_label,
        handler,
        initialization_scripts,
        is_main_frame,
        initiator_origin,
        response: Arc::new(RefCell::new(None)),
      }))
    }
  }
}

struct ThreadSafe<T>(T);

impl<T> ThreadSafe<T> {
  fn into_owned(self) -> T {
    self.0
  }
}

unsafe impl<T> Send for ThreadSafe<T> {}
unsafe impl<T> Sync for ThreadSafe<T> {}

fn read_request_body(request: &mut Request) -> Vec<u8> {
  let mut body = Vec::new();

  if let Some(post_data) = request.post_data() {
    let mut elements = vec![None; post_data.element_count()];
    post_data.elements(Some(&mut elements));
    for element in elements.into_iter().flatten() {
      match element.get_type().as_ref() {
        sys::cef_postdataelement_type_t::PDE_TYPE_BYTES => {
          let size = element.bytes_count();
          if size > 0 {
            let mut buf = vec![0u8; size];
            // Copy bytes into our buffer
            let copied = element.bytes(size, buf.as_mut_ptr());
            // Safety: CEF promises it wrote `copied` bytes into buf
            unsafe {
              buf.set_len(copied);
            }
            body.extend(buf);
          }
        }
        sys::cef_postdataelement_type_t::PDE_TYPE_FILE => {
          // Read file from disk
          let file_path = CefString::from(&element.file()).to_string();
          if let Ok(mut file) = std::fs::File::open(&file_path) {
            use std::io::Read;
            let mut buf = Vec::new();
            if file.read_to_end(&mut buf).is_ok() {
              body.extend(buf);
            }
          }
        }
        _ => {}
      }
    }
  }

  body
}

fn get_request_headers(request: &mut Request) -> HeaderMap {
  let mut headers = HeaderMap::new();

  let mut map = CefStringMultimap::new();

  request.header_map(Some(&mut map));

  // Iterate through all entries
  for (name, value) in map {
    for v in value {
      headers.append(
        HeaderName::from_bytes(name.as_bytes()).unwrap(),
        HeaderValue::from_str(&v).unwrap(),
      );
    }
  }

  headers
}

// ==== Renderer termination boundary ====

fn termination_reason(
  status: TerminationStatus,
) -> tauri_runtime::webview::WebContentProcessTerminationReason {
  use tauri_runtime::webview::WebContentProcessTerminationReason as Reason;
  match status {
    TerminationStatus::ABNORMAL_TERMINATION => Reason::Abnormal,
    TerminationStatus::PROCESS_WAS_KILLED => Reason::Killed,
    TerminationStatus::PROCESS_CRASHED => Reason::Crashed,
    TerminationStatus::PROCESS_OOM => Reason::OutOfMemory,
    TerminationStatus::LAUNCH_FAILED => Reason::LaunchFailed,
    TerminationStatus::INTEGRITY_FAILURE => Reason::IntegrityFailure,
    _ => Reason::Unknown,
  }
}

#[cfg(test)]
mod protocol_dispatch_tests {
  use super::*;
  use std::{
    collections::HashSet,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
  };

  const WAIT: Duration = Duration::from_secs(2);

  #[test]
  fn ipc_preflight_does_not_need_an_available_worker_and_does_not_dispatch_a_command() {
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let handler: Arc<Box<UriSchemeProtocolHandler>> = Arc::new(Box::new(move |_, _, _| {
      called.fetch_add(1, Ordering::SeqCst);
    }));
    for uri in [
      "http://ipc.localhost/test",
      "https://ipc.localhost/test",
      "ipc://localhost/test",
    ] {
      let request = http::Request::builder()
        .method(http::Method::OPTIONS)
        .uri(uri)
        .body(vec![])
        .unwrap();
      let (reply, reply_rx) = mpsc::channel();
      dispatch_protocol(
        None,
        handler.clone(),
        "main".into(),
        request,
        Box::new(move |response| {
          reply.send(response).unwrap();
        }),
      );
      let response = reply_rx.recv_timeout(WAIT).unwrap();
      assert_eq!(response.status(), http::StatusCode::OK);
      assert_eq!(response.headers()["access-control-allow-origin"], "*");
      assert_eq!(response.headers()["access-control-allow-headers"], "*");
      assert_eq!(
        response.headers()["access-control-allow-methods"],
        "POST, OPTIONS"
      );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    for uri in [
      "http://tauri.localhost/test",
      "https://example.com/test",
      "custom://localhost/test",
    ] {
      let request = http::Request::builder()
        .method(http::Method::OPTIONS)
        .uri(uri)
        .body(vec![])
        .unwrap();
      assert!(ipc_preflight_response(&request).is_none());
    }
    let request = http::Request::builder()
      .method(http::Method::POST)
      .uri("http://ipc.localhost/test")
      .body(vec![])
      .unwrap();
    assert!(ipc_preflight_response(&request).is_none());
  }

  #[test]
  fn request_pressure_is_bounded_and_reuses_the_same_workers() {
    let executor = RequestExecutor::new(2, 3).unwrap();
    let (started, started_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    for _ in 0..2 {
      let started = started.clone();
      let release_rx = release_rx.clone();
      executor
        .try_dispatch(Box::new(move || {
          started.send(std::thread::current().id()).unwrap();
          let _ = release_rx.lock().unwrap().recv();
        }))
        .ok()
        .unwrap();
    }
    let workers: HashSet<_> = (0..2)
      .map(|_| started_rx.recv_timeout(WAIT).unwrap())
      .collect();
    assert_eq!(workers.len(), 2);

    let (finished, finished_rx) = mpsc::channel();
    for _ in 0..3 {
      let finished = finished.clone();
      executor
        .try_dispatch(Box::new(move || {
          finished.send(std::thread::current().id()).unwrap();
        }))
        .ok()
        .unwrap();
    }
    let rejected_executions = Arc::new(AtomicUsize::new(0));
    for _ in 0..1_024 {
      let executions = rejected_executions.clone();
      assert!(matches!(
        executor.try_dispatch(Box::new(move || {
          executions.fetch_add(1, Ordering::SeqCst);
        })),
        Err(mpsc::TrySendError::Full(_))
      ));
    }
    release.send(()).unwrap();
    release.send(()).unwrap();
    for _ in 0..3 {
      assert!(workers.contains(&finished_rx.recv_timeout(WAIT).unwrap()));
    }
    assert_eq!(rejected_executions.load(Ordering::SeqCst), 0);
  }

  #[test]
  fn saturated_dispatch_replies_once_without_running_the_handler_or_fetch_fallback() {
    let executor = RequestExecutor::new(1, 1).unwrap();
    let (started, started_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    executor
      .try_dispatch(Box::new(move || {
        started.send(()).unwrap();
        let _ = release_rx.recv();
      }))
      .ok()
      .unwrap();
    started_rx.recv_timeout(WAIT).unwrap();
    executor.try_dispatch(Box::new(|| {})).ok().unwrap();

    let executions = Arc::new(AtomicUsize::new(0));
    let executed = executions.clone();
    let handler: Arc<Box<UriSchemeProtocolHandler>> = Arc::new(Box::new(move |_, _, _| {
      executed.fetch_add(1, Ordering::SeqCst);
    }));
    let (reply, reply_rx) = mpsc::channel();
    dispatch_protocol(
      Some(&executor),
      handler,
      "main".into(),
      http::Request::new(vec![]),
      Box::new(move |response| {
        reply.send(response).unwrap();
      }),
    );
    let response = reply_rx.recv_timeout(WAIT).unwrap();
    assert_eq!(response.status(), http::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()["tauri-response"], "error");
    assert_eq!(response.headers()["access-control-allow-origin"], "*");
    assert_eq!(
      response.headers()["access-control-expose-headers"],
      "Tauri-Response"
    );
    assert!(serde_json::from_slice::<String>(response.body())
      .unwrap()
      .contains("busy"));
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert!(reply_rx.try_recv().is_err());
    release.send(()).unwrap();
  }

  #[test]
  fn asynchronous_responses_do_not_hold_a_dispatch_worker() {
    let executor = RequestExecutor::new(1, 1).unwrap();
    let (held, held_rx) = mpsc::channel::<ProtocolResponder>();
    let handler: Arc<Box<UriSchemeProtocolHandler>> = Arc::new(Box::new(move |_, _, response| {
      held.send(response).unwrap();
    }));
    let (reply, reply_rx) = mpsc::channel();
    let mut deferred = Vec::new();
    for _ in 0..2 {
      let reply = reply.clone();
      dispatch_protocol(
        Some(&executor),
        handler.clone(),
        "main".into(),
        http::Request::new(vec![]),
        Box::new(move |response| {
          reply.send(response.status()).unwrap();
        }),
      );
      // The second handler must run before the first has answered.
      deferred.push(held_rx.recv_timeout(WAIT).unwrap());
    }
    assert!(reply_rx.try_recv().is_err());
    for responder in deferred {
      responder(http::Response::new(Cow::Borrowed(b"ok")));
    }
    for _ in 0..2 {
      assert_eq!(reply_rx.recv_timeout(WAIT).unwrap(), http::StatusCode::OK);
    }
  }

  #[test]
  fn failed_worker_start_and_panicking_handler_return_an_error_without_losing_the_pool() {
    let executor = RequestExecutor::new(1, 1).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let handler: Arc<Box<UriSchemeProtocolHandler>> = Arc::new(Box::new(move |_, _, response| {
      if called.fetch_add(1, Ordering::SeqCst) == 0 {
        panic!("fixture handler failed");
      }
      response(http::Response::new(Cow::Borrowed(b"ok")));
    }));
    let (reply, reply_rx) = mpsc::channel();
    for (pool, expected) in [
      (None, http::StatusCode::SERVICE_UNAVAILABLE),
      (Some(&executor), http::StatusCode::INTERNAL_SERVER_ERROR),
      (Some(&executor), http::StatusCode::OK),
    ] {
      let reply = reply.clone();
      dispatch_protocol(
        pool,
        handler.clone(),
        "main".into(),
        http::Request::new(vec![]),
        Box::new(move |response| {
          reply.send(response.status()).unwrap();
        }),
      );
      assert_eq!(reply_rx.recv_timeout(WAIT).unwrap(), expected);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
  }
}

#[cfg(test)]
mod termination_tests {
  use super::*;
  use tauri_runtime::webview::WebContentProcessTerminationReason as Reason;

  #[test]
  fn preserves_every_cef_termination_reason() {
    for (status, expected) in [
      (TerminationStatus::ABNORMAL_TERMINATION, Reason::Abnormal),
      (TerminationStatus::PROCESS_WAS_KILLED, Reason::Killed),
      (TerminationStatus::PROCESS_CRASHED, Reason::Crashed),
      (TerminationStatus::PROCESS_OOM, Reason::OutOfMemory),
      (TerminationStatus::LAUNCH_FAILED, Reason::LaunchFailed),
      (
        TerminationStatus::INTEGRITY_FAILURE,
        Reason::IntegrityFailure,
      ),
      (TerminationStatus::NUM_VALUES, Reason::Unknown),
    ] {
      assert_eq!(termination_reason(status), expected);
    }
  }
}
