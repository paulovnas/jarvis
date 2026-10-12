//! Synthetic probe and opt-in read-only observation of real Companion chrome.
use super::*;
use objc2::{rc::Retained, AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSColor, NSEvent, NSEventModifierFlags, NSEventType,
    NSView,
};
use objc2_foundation::{NSDictionary, NSPoint, NSString};

pub(super) const PAGE: &str = r#"<!doctype html><html class="dark" data-companion><head><title>Island probe</title><style>
html,body,#root{margin:0;background:transparent;font:16px system-ui;color:white}
.dark{color-scheme:dark}
main{position:absolute;inset:24px;background:rgb(32,36,43);border-radius:24px;padding:24px;box-sizing:border-box}
textarea{width:180px;height:48px}button{margin-top:8px;display:block}
.blend{position:absolute;left:4px;top:102px;width:16px;height:16px;background:rgba(97,175,239,.5)}
canvas{position:absolute;right:16px;bottom:12px;width:80px;height:80px;transform-origin:top left;transform:scale(.35);pointer-events:none}
</style></head><body><div id="root"><div class="blend"></div><main><textarea id="text" aria-label="Texto"></textarea><button id="action">Testar</button><p id="result">Pronto</p><canvas width="80" height="80" aria-hidden="true"></canvas></main></div><script>
window.__inputTrusted=false;window.__clickTrusted=false;
document.querySelector('#text').addEventListener('input',event=>window.__inputTrusted=event.isTrusted);
document.querySelector('#action').addEventListener('click',event=>{window.__clickTrusted=event.isTrusted;document.querySelector('#result').textContent='Olá '+document.querySelector('#text').value;});
const canvas=document.querySelector('canvas');
const context=canvas.getContext('2d');context.fillStyle='rgb(97,175,239)';context.fillRect(0,0,80,80);
window.__startIslandAnimation=()=>{
  const card=document.querySelector('main');
  const start=performance.now();let previous=start;
  const state=window.__islandAnimation={done:false,completedTransitions:0,frames:0,canvasStable:true,minRenderedWidth:80,maxRenderedWidth:0,maxFrameMs:0,durationMs:0};
  const frame=timestamp=>{
    const progress=Math.min(1,Math.max(0,(timestamp-start)/1000));
    const phase=Math.sin(progress*Math.PI);
    card.style.transform=`scaleY(${1-phase*.25})`;
    canvas.style.transform=`translate3d(${phase*12}px,0,0) scale(${.35+phase*.65})`;
    state.frames++;state.maxFrameMs=Math.max(state.maxFrameMs,timestamp-previous);previous=timestamp;
    state.canvasStable=state.canvasStable&&canvas.width===80&&canvas.height===80&&canvas.clientWidth===80&&canvas.clientHeight===80;
    const width=canvas.getBoundingClientRect().width;
    state.minRenderedWidth=Math.min(state.minRenderedWidth,width);state.maxRenderedWidth=Math.max(state.maxRenderedWidth,width);
    if(progress<1){requestAnimationFrame(frame);}else{
      card.style.transform='';canvas.style.transform='scale(.35)';
      state.done=true;state.completedTransitions=1;state.durationMs=timestamp-start;
    }
  };
  requestAnimationFrame(frame);return true;
};
</script></body></html>"#;
const TEXT: &str = "Olá, ação";
const EVIDENCE: &str = "({value:document.querySelector('#text').value,inputTrusted:window.__inputTrusted,clickTrusted:window.__clickTrusted,result:document.querySelector('#result').textContent})";

fn surface(root: &NSView) -> Option<Retained<NSView>> {
    if root.class().name().to_bytes() == b"JarvisCefOffscreenView" {
        // The caller holds its native window and executes on AppKit's main thread.
        return unsafe { Retained::retain(root as *const NSView as *mut NSView) };
    }
    root.subviews().iter().find_map(|child| surface(&child))
}

async fn native<F, T>(window: &tauri::WebviewWindow, action: F) -> Result<T, AgentError>
where
    F: FnOnce(&NSView) -> Result<T, AgentError> + Send + 'static,
    T: Send + 'static,
{
    let window = window.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    dispatch2::DispatchQueue::main().exec_async(move || {
        let result = (|| {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| error("Island native observation is not on AppKit's thread"))?;
            let root = crate::companion::probe::content_view(window.app_handle(), mtm)
                .map_err(|cause| error(&cause))?;
            let view =
                surface(&root).ok_or_else(|| error("Island is not a windowless CEF surface"))?;
            action(&view)
        })();
        let _ = tx.send(result);
    });
    tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| error("Island native observation timed out"))?
        .map_err(|_| error("Island closed during native observation"))?
}

fn alpha_valid(evidence: &Value) -> bool {
    evidence["cornerAlpha"]
        .as_f64()
        .is_some_and(|alpha| alpha < 0.01)
        && evidence["blendAlpha"]
            .as_f64()
            .is_some_and(|alpha| (0.49..0.51).contains(&alpha))
        && evidence["cardAlpha"]
            .as_f64()
            .is_some_and(|alpha| alpha > 0.99)
        && evidence["viewOpaque"] == false
        && evidence["windowOpaque"] == false
        && evidence["windowClass"] == "JarvisCompanionPanel"
        && evidence["windowBackgroundAlpha"]
            .as_f64()
            .is_some_and(|alpha| alpha < 0.01)
}

fn bitmap(view: &NSView) -> Result<Retained<NSBitmapImageRep>, AgentError> {
    let layer = view
        .layer()
        .ok_or_else(|| error("Island has no backing layer"))?;
    // The identified OSR class owns a CGImage in CALayer.contents, not a screenshot
    // or a CDP background override. AppKit reads the actual displayed pixel alpha.
    let contents = unsafe { layer.contents() }.ok_or_else(|| error("Island has not painted"))?;
    Ok(unsafe {
        NSBitmapImageRep::initWithCGImage(
            NSBitmapImageRep::alloc(),
            &*std::ptr::from_ref(&*contents).cast(),
        )
    })
}

fn png(view: &NSView) -> Result<Vec<u8>, AgentError> {
    let image = bitmap(view)?;
    // An empty properties dictionary requests AppKit's lossless PNG defaults.
    unsafe {
        image.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .map(|data| data.to_vec())
    .ok_or_else(|| error("Could not encode the island native paint as PNG"))
}

fn alpha(view: &NSView) -> Result<Value, AgentError> {
    alpha_at(
        view,
        &[
            ("cornerAlpha", 4., 4.),
            ("blendAlpha", 12., 110.),
            ("cardAlpha", 180., 36.),
        ],
    )
}

fn alpha_at(view: &NSView, points: &[(&str, f64, f64)]) -> Result<Value, AgentError> {
    let image = bitmap(view)?;
    let size = view.bounds().size;
    let window = view
        .window()
        .ok_or_else(|| error("Island has no native window"))?;
    let mut evidence = json!({"viewOpaque":view.isOpaque(),"windowOpaque":window.isOpaque(),"windowVisible":window.isVisible(),"windowClass":window.class().name().to_string_lossy(),"windowBackgroundAlpha":window.backgroundColor().alphaComponent(),"pixelWidth":image.pixelsWide(),"pixelHeight":image.pixelsHigh(),"viewWidth":size.width,"viewHeight":size.height});
    let content = window.contentView();
    // Native observation owns this live surface and runs on AppKit's thread.
    let mut current = unsafe { Retained::retain(view as *const NSView as *mut NSView) };
    let mut ancestors = Vec::new();
    for _ in 0..8 {
        let Some(node) = current else {
            break;
        };
        let is_content = content
            .as_ref()
            .is_some_and(|content| Retained::as_ptr(content) == Retained::as_ptr(&node));
        let layer = node.layer().map(|layer| {
            let background = layer
                .backgroundColor()
                .and_then(|color| NSColor::colorWithCGColor(&color))
                .map(|color| color.alphaComponent());
            json!({"opaque":layer.isOpaque(),"backgroundAlpha":background})
        });
        let bounds = node.bounds();
        ancestors.push(json!({"class":node.class().name().to_string_lossy(),"opaque":node.isOpaque(),"hidden":node.isHiddenOrHasHiddenAncestor(),"contentRoot":is_content,"width":bounds.size.width,"height":bounds.size.height,"layer":layer}));
        if is_content {
            break;
        }
        current = unsafe { node.superview() };
    }
    evidence["nativeViews"] = json!(ancestors);
    for (name, x, y) in points {
        let color = image
            .colorAtX_y(
                (x * image.pixelsWide() as f64 / size.width) as isize,
                (y * image.pixelsHigh() as f64 / size.height) as isize,
            )
            .ok_or_else(|| error("Island pixel is unavailable"))?;
        evidence[*name] = json!(color.alphaComponent());
    }
    Ok(evidence)
}

const OBSERVE_DOM: &str = r#"(async () => {
  const element = selector => {
    const node = document.querySelector(selector);
    if (!node) return null;
    const style = getComputedStyle(node), rect = node.getBoundingClientRect();
    return { backgroundColor:style.backgroundColor, colorScheme:style.colorScheme, borderRadius:style.borderRadius, opacity:style.opacity, hasBackgroundImage:style.backgroundImage!=='none', width:rect.width, height:rect.height, x:rect.x, y:rect.y };
  };
  const companion = document.querySelector('.companion');
  const dom = { ready:document.readyState, mainFrame:window===window.top, companionAttribute:document.documentElement.hasAttribute('data-companion'), dark:document.documentElement.classList.contains('dark'), hidden:document.hidden, expanded:companion?.dataset.expanded==='true', html:element('html'), body:element('body'), root:element('#root'), companion:element('.companion'), island:element('.companion-island'), canvasCount:document.querySelectorAll('canvas').length };
  let frames=0, previous, maxGapMs=0, done=false;
  const start=performance.now();
  const frame=now=>{ if(done)return; frames++; if(previous!==undefined)maxGapMs=Math.max(maxGapMs,now-previous); previous=now; requestAnimationFrame(frame); };
  requestAnimationFrame(frame);
  return new Promise(resolve=>setTimeout(()=>{done=true;resolve({...dom,frames,maxGapMs,frameDurationMs:performance.now()-start});},250));
})()"#;

pub(super) async fn observe_companion(app: &tauri::AppHandle) -> Result<(), AgentError> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.codex/browser-review");
    std::fs::create_dir_all(&directory).map_err(|_| AgentError::storage())?;
    let output = directory.join("real-island-observation.json");
    let start = std::time::Instant::now();
    let mut samples = Vec::new();
    let mut missing = 0;
    let mut unavailable = 0;
    let mut observed = 0;
    println!(
        "Observing real Companion paint for 40 seconds: {}",
        output.display()
    );
    while start.elapsed() < Duration::from_secs(40) {
        if let (Some(window), Some(view)) = (
            app.get_webview_window("companion"),
            app.get_webview("companion"),
        ) {
            let dom =
                cdp_action(&view, json!({"action":"evaluate","expression":OBSERVE_DOM})).await;
            let paint = native(&window, |view| {
                let size = view.bounds().size;
                let right = (size.width - 1.).max(0.);
                let bottom = (size.height - 1.).max(0.);
                alpha_at(
                    view,
                    &[
                        ("topLeftAlpha", 0., 0.),
                        ("topRightAlpha", right, 0.),
                        ("bottomLeftAlpha", 0., bottom),
                        ("bottomRightAlpha", right, bottom),
                        ("centerAlpha", size.width / 2., size.height / 2.),
                    ],
                )
            })
            .await;
            if dom.is_ok() && paint.is_ok() {
                observed += 1;
            } else {
                unavailable += 1;
            }
            samples.push(
                json!({"elapsedMs":start.elapsed().as_millis(),"dom":dom.ok(),"paint":paint.ok()}),
            );
        } else {
            missing += 1;
        }
        let report = json!({"normalStartup":true,"readOnly":true,"elapsedMs":start.elapsed().as_millis(),"missingViewSamples":missing,"unavailableSamples":unavailable,"observedFrames":observed,"samples":samples});
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(&report).map_err(|_| AgentError::internal())?,
        )
        .map_err(|_| AgentError::storage())?;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if observed == 0 {
        return Err(error("Real Companion produced no observable frames"));
    }
    println!(
        "Real Companion observation complete: {} samples",
        samples.len()
    );
    Ok(())
}

fn click(view: &NSView, x: f64, y: f64) -> Result<(), AgentError> {
    let window = view
        .window()
        .ok_or_else(|| error("Island has no native window"))?;
    let y = if view.isFlipped() {
        y
    } else {
        view.bounds().size.height - y
    };
    let location = view.convertPoint_toView(NSPoint::new(x, y), None);
    for kind in [NSEventType::LeftMouseDown, NSEventType::LeftMouseUp] {
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind, location, NSEventModifierFlags::empty(), 0., window.windowNumber(), None, 0, 1, 1.,
        ).ok_or_else(|| error("Could not construct island mouse event"))?;
        if kind == NSEventType::LeftMouseDown {
            view.mouseDown(&event);
        } else {
            view.mouseUp(&event);
        }
    }
    Ok(())
}

fn type_text(view: &NSView) -> Result<(), AgentError> {
    let window = view
        .window()
        .ok_or_else(|| error("Island has no native window"))?;
    for character in TEXT.chars() {
        let text = NSString::from_str(&character.to_string());
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind, NSPoint::new(0., 0.), NSEventModifierFlags::empty(), 0., window.windowNumber(), None, &text, &text, false, 0,
            ).ok_or_else(|| error("Could not construct island keyboard event"))?;
            if kind == NSEventType::KeyDown {
                view.keyDown(&event);
            } else {
                view.keyUp(&event);
            }
        }
    }
    Ok(())
}

fn input_valid(evidence: &Value) -> bool {
    evidence["value"] == TEXT
        && evidence["inputTrusted"] == true
        && evidence["clickTrusted"] == true
        && evidence["result"] == format!("Olá {TEXT}")
}

fn animation_valid(state: &Value, min_alpha: f64, max_alpha: f64) -> bool {
    state["done"] == true
        && state["completedTransitions"] == 1
        && state["canvasStable"] == true
        && state["frames"].as_u64().is_some_and(|frames| frames >= 4)
        && state["minRenderedWidth"]
            .as_f64()
            .is_some_and(|width| width <= 30.)
        && state["maxRenderedWidth"]
            .as_f64()
            .is_some_and(|width| width >= 70.)
        && min_alpha < 0.01
        && max_alpha > 0.99
}

async fn animate(window: &tauri::WebviewWindow, view: &Webview) -> Result<Value, AgentError> {
    evaluate(view, "window.__startIslandAnimation()".into()).await?;
    let (mut min_alpha, mut max_alpha) = (1_f64, 0_f64);
    let mut samples = 0;
    let mut state = Value::Null;
    for _ in 0..60 {
        if let Some(observed_alpha) = native(window, alpha).await?["cardAlpha"].as_f64() {
            min_alpha = min_alpha.min(observed_alpha);
            max_alpha = max_alpha.max(observed_alpha);
            samples += 1;
        }
        state = evaluate(view, "window.__islandAnimation".into()).await?;
        if state["done"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    if !animation_valid(&state, min_alpha, max_alpha) {
        return Err(error(&format!(
            "Island animation failed: {state}; native alpha range {min_alpha}..{max_alpha}"
        )));
    }
    // The card shrinks around its center, so both top and bottom pixels leave it;
    // this observation does not depend on AppKit bitmap-row orientation. Pointer
    // targets are queried only after the final untransformed paint returns.
    for _ in 0..40 {
        if alpha_valid(&native(window, alpha).await?) {
            return Ok(
                json!({"raf":state,"nativePaintSamples":samples,"minCardAlpha":min_alpha,"maxCardAlpha":max_alpha}),
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(error(
        "Island animation did not restore its final native paint",
    ))
}

pub(super) async fn check(
    app: &tauri::AppHandle,
    base: &str,
    output: &std::path::Path,
) -> Result<(), AgentError> {
    let main = app
        .get_webview("main")
        .ok_or_else(|| error("Probe main view is unavailable"))?;
    evaluate(&main, "(() => { window.__islandProbeNonce='preserved'; document.body.style.background='rgb(41,46,55)'; return true; })()".into()).await?;
    let mut cycles = Vec::new();
    for cycle in 1..=2 {
        let phase = if cycle == 1 { "creating" } else { "reopening" };
        println!("Probe: island cycle {cycle}: {phase} native panel");
        let window = tauri::WebviewWindowBuilder::new(
            app,
            "island-probe",
            WebviewUrl::External(address(&format!("{base}/island"))?),
        )
        .title("Jarvis island integration probe")
        .inner_size(360., 220.)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::utils::config::Color(0, 0, 0, 0))
        .visible(false)
        .focused(false)
        .shadow(false)
        .build()
        .map_err(|_| error("Could not create island probe window"))?;
        crate::companion::probe::show(&window).map_err(|cause| error(&cause))?;
        let view = app
            .get_webview("island-probe")
            .ok_or_else(|| error("Island webview is unavailable"))?;
        wait_page(&view, "Island probe").await?;
        let mut pixels = Value::Null;
        for _ in 0..40 {
            match native(&window, alpha).await {
                Ok(value) => pixels = value,
                Err(cause) => pixels = json!({"error": cause.message}),
            }
            if alpha_valid(&pixels) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !alpha_valid(&pixels) {
            return Err(error(&format!("Island native alpha failed: {pixels}")));
        }
        println!("Probe: island cycle {cycle}: native alpha verified");
        let animation = animate(&window, &view).await?;
        let positions = evaluate(&view, "['#text','#action'].map(selector=>{const rect=document.querySelector(selector).getBoundingClientRect();return [rect.x+rect.width/2,rect.y+rect.height/2]})".into()).await?;
        let points: [[f64; 2]; 2] = serde_json::from_value(positions)
            .map_err(|_| error("Island input targets unavailable"))?;
        native(&window, move |surface| {
            click(surface, points[0][0], points[0][1])?;
            Ok(Value::Null)
        })
        .await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        native(&window, |surface| {
            type_text(surface)?;
            Ok(Value::Null)
        })
        .await?;
        native(&window, move |surface| {
            click(surface, points[1][0], points[1][1])?;
            Ok(Value::Null)
        })
        .await?;
        let mut input = Value::Null;
        for _ in 0..40 {
            input = evaluate(&view, EVIDENCE.into()).await?;
            if input_valid(&input) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !input_valid(&input) {
            return Err(error(&format!("Island native input failed: {input}")));
        }
        println!("Probe: island cycle {cycle}: trusted native input verified");
        std::fs::write(
            output.join(format!("native-island-{cycle}.png")),
            native(&window, png).await?,
        )
        .map_err(|_| AgentError::storage())?;
        println!("Probe: island cycle {cycle}: closing native panel");
        crate::companion::probe::close(&window).map_err(|cause| error(&cause))?;
        for _ in 0..80 {
            if app.get_webview("island-probe").is_none() && app.get_window("island-probe").is_none()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if app.get_webview("island-probe").is_some() || app.get_window("island-probe").is_some() {
            return Err(error("Island close left a registered window or browser"));
        }
        cdp::forget(&app.state::<BrowserState>(), view.label());
        println!("Probe: island cycle {cycle}: native panel closed");
        if evaluate(&main, "window.__islandProbeNonce".into()).await? != "preserved" {
            return Err(error(
                "Closing island replaced or reloaded the main browser",
            ));
        }
        cycles.push(json!({"cycle":cycle,"nativePixels":pixels,"animation":animation,"nativeInput":input,"closed":true,"mainPreserved":true}));
    }
    std::fs::write(output.join("native-island-report.json"), serde_json::to_vec_pretty(&json!({"passed":true,"checks":["native CALayer alpha","real Companion NSPanel reparenting","transparent native view and window","animated native paint and stable canvas dimensions","trusted native pointer and accented keyboard","close and reopen twice","main browser preserved"],"cycles":cycles})).unwrap())
        .map_err(|_| AgentError::storage())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_requires_completed_visible_motion_and_stable_canvas() {
        let state = json!({"done":true,"completedTransitions":1,"canvasStable":true,"frames":60,"minRenderedWidth":28.,"maxRenderedWidth":80.});
        assert!(animation_valid(&state, 0., 1.));
        assert!(!animation_valid(&state, 1., 1.));
        for (field, value) in [
            ("done", json!(false)),
            ("completedTransitions", json!(0)),
            ("canvasStable", json!(false)),
            ("frames", json!(1)),
            ("minRenderedWidth", json!(80.)),
            ("maxRenderedWidth", json!(28.)),
        ] {
            let mut invalid = state.clone();
            invalid[field] = value;
            assert!(!animation_valid(&invalid, 0., 1.));
        }
    }

    #[test]
    fn alpha_rejects_black_or_opaque_native_surfaces() {
        let pixels = json!({"cornerAlpha":0.,"blendAlpha":0.5,"cardAlpha":1.,"viewOpaque":false,"windowOpaque":false,"windowClass":"JarvisCompanionPanel","windowBackgroundAlpha":0.});
        assert!(alpha_valid(&pixels));
        for (field, value) in [
            ("cornerAlpha", json!(1.)),
            ("blendAlpha", json!(1.)),
            ("cardAlpha", json!(0.)),
            ("viewOpaque", json!(true)),
            ("windowOpaque", json!(true)),
            ("windowClass", json!("NSWindow")),
            ("windowBackgroundAlpha", json!(1.)),
        ] {
            let mut invalid = pixels.clone();
            invalid[field] = value;
            assert!(!alpha_valid(&invalid));
        }
        assert!(!alpha_valid(&Value::Null));
    }

    #[test]
    fn native_input_requires_trusted_accented_text_and_click() {
        let input = json!({"value":TEXT,"inputTrusted":true,"clickTrusted":true,"result":format!("Olá {TEXT}")});
        assert!(input_valid(&input));
        for (field, value) in [
            ("value", json!("Ola, acao")),
            ("inputTrusted", json!(false)),
            ("clickTrusted", json!(false)),
            ("result", json!("Pronto")),
        ] {
            let mut invalid = input.clone();
            invalid[field] = value;
            assert!(!input_valid(&invalid));
        }
    }
}
