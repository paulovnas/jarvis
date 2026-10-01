//! Opt-in real installer, inference, shared timing and MP4 delivery regression.
use super::*;
use std::sync::Arc;

struct NativeTools<'a> {
    home: &'a Path,
    session: Arc<super::super::Session>,
    jobs: Jobs,
    commands: CommandSessions,
    signal: watch::Receiver<bool>,
}

fn tool(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: name.into(),
        name: name.into(),
        args,
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

fn sandbox(root: &Path, call: &ToolCall) -> SandboxPlan {
    let catalog = super::super::tool_contract::Catalog::new(&definitions(Mode::Build));
    let policy = super::super::execution_policy::inspect_tool(
        root,
        call,
        catalog.capabilities(&call.name).unwrap(),
    )
    .unwrap()
    .unwrap();
    let plan = super::super::execution_sandbox::prepare(&policy).unwrap();
    #[cfg(target_os = "macos")]
    assert_eq!(
        plan.report().backend,
        super::super::execution_sandbox::SandboxBackend::MacosSeatbelt
    );
    plan
}

impl NativeTools<'_> {
    async fn execute(&mut self, name: &str, args: Value) -> Value {
        let call = tool(name, args);
        let plan =
            matches!(name, "video_audio" | "video_run").then(|| sandbox(&self.session.root, &call));
        let mut result: Value = serde_json::from_str(
            &self
                .jobs
                .execute(
                    &mut self.commands,
                    Context {
                        session: &self.session,
                        home: self.home,
                        app: None,
                    },
                    &call,
                    plan.as_ref(),
                    self.signal.clone(),
                )
                .await
                .unwrap_or_else(|failure| panic!("{name}: {failure:?}")),
        )
        .unwrap();
        if name == "video_presentation" {
            return result;
        }
        let session_id = result["sessionId"].clone();
        let mut output = result["output"].as_str().unwrap_or_default().to_owned();
        while result["status"] == "running" {
            let wait = tool(
                "video_wait",
                json!({"sessionId":session_id,"cursor":result["cursor"],"yieldTimeMs":30000}),
            );
            result = serde_json::from_str(
                &self
                    .jobs
                    .execute(
                        &mut self.commands,
                        Context {
                            session: &self.session,
                            home: self.home,
                            app: None,
                        },
                        &wait,
                        plan.as_ref(),
                        self.signal.clone(),
                    )
                    .await
                    .unwrap_or_else(|failure| panic!("{name} wait: {failure:?}")),
            )
            .unwrap();
            assert_eq!(result["sessionId"], session_id);
            output.push_str(result["output"].as_str().unwrap_or_default());
        }
        assert_eq!(result["status"], "completed", "{result}");
        assert_eq!(result["exitCode"], 0, "{result}");
        eprintln!("Native {name}: {output}");
        result
    }
}

#[tokio::test]
#[ignore = "Requires an installed private Hyperframes runtime; exercises actual OS sandbox rendering"]
async fn managed_video_renders_inside_production_sandbox() {
    let home = std::env::var_os("JARVIS_TEST_VIDEO_HOME")
        .expect("Set JARVIS_TEST_VIDEO_HOME to a home with managed Hyperframes installed");
    let home = Path::new(&home);
    crate::core::hyperframes::runtime(home).unwrap();
    let fixture = super::super::tests::Fixture::new();
    let (_stop, signal) = watch::channel(false);
    let mut native = NativeTools {
        home,
        session: super::super::tests::session(&fixture),
        jobs: Jobs::default(),
        commands: CommandSessions::default(),
        signal,
    };
    let directory = fixture.root.join("smoke");
    if let Some(source) = std::env::var_os("JARVIS_TEST_VIDEO_COMPOSITION") {
        copy_composition(Path::new(&source), &directory);
    } else {
        native
            .execute("video_run", json!({"action":"init","path":"smoke"}))
            .await;
        fs::create_dir(directory.join("compositions")).unwrap();
        fs::write(directory.join("compositions/intro.html"), r##"<!doctype html><html><head><meta charset="UTF-8"></head><body><template><style>#intro-root{position:absolute;inset:0;width:320px;height:180px;color:white;font-family:sans-serif}</style><div id="intro-root" data-composition-id="sandbox-intro" data-width="320" data-height="180" data-duration="1"><div id="title">Jarvis sandbox render</div></div><script>const child=gsap.timeline({paused:true});child.to("#title",{x:20,duration:1,ease:"none"},0);window.__timelines["sandbox-intro"]=child;</script></template></body></html>"##).unwrap();
        fs::write(directory.join("index.html"), r##"<!doctype html><html><head><meta charset="UTF-8"><script src="assets/vendor/gsap.min.js"></script><style>html,body{margin:0;width:320px;height:180px;overflow:hidden;background:#181b20}</style></head><body><main data-composition-id="sandbox-smoke" data-width="320" data-height="180" data-start="0" data-duration="1"><div id="intro" class="clip" data-composition-id="sandbox-intro" data-composition-src="compositions/intro.html" data-width="320" data-height="180" data-start="0" data-duration="1" data-track-index="0"></div></main><script>window.__timelines={};window.__timelines["sandbox-smoke"]=gsap.timeline({paused:true}).to({},{duration:1},0);</script></body></html>"##).unwrap();
    }
    native
        .execute("video_run", json!({"action":"check","path":"smoke"}))
        .await;
    let rendered = native
        .execute(
            "video_run",
            json!({"action":"render","path":"smoke","quality":"draft","output":"smoke/ready.mp4"}),
        )
        .await;
    assert_eq!(rendered["path"], "smoke/ready.mp4");
    verify_video(&fixture.root, "smoke/ready.mp4").unwrap();
    assert!(native.commands.running_ids().is_empty());
    if std::env::var_os("JARVIS_TEST_VIDEO_COMPOSITION").is_some() {
        let artifact = tempfile::Builder::new()
            .prefix("jarvis-native-video-")
            .suffix(".mp4")
            .tempfile()
            .unwrap();
        fs::copy(directory.join("ready.mp4"), artifact.path()).unwrap();
        let (_, path) = artifact.keep().unwrap();
        eprintln!("NATIVE_SANDBOX_MP4={}", path.display());
    }
}

fn copy_composition(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(
            !kind.is_symlink(),
            "Composition fixtures must not contain links"
        );
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_composition(&entry.path(), &target);
        } else if kind.is_file() {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[cfg(target_os = "macos")]
async fn verify_host_lock_scope(root: &Path, home: &Path) {
    // /tmp is intentionally writable in Seatbelt. Probe outside it, proving
    // the host's exact-file grant rather than accidentally testing that rule.
    let host_home = std::env::var_os("HOME").unwrap();
    let directory = tempfile::Builder::new()
        .prefix(".jarvis-audiovisual-lock-smoke-")
        .tempdir_in(host_home)
        .unwrap();
    let lock = directory.path().join(".inference.lock");
    fs::write(&lock, b"").unwrap();
    let lock = lock.canonicalize().unwrap();
    let runtime = crate::core::audiovisual::runtime(home).unwrap();
    let call = tool("video_audio", json!({"action":"narrate"}));
    let plan = sandbox(root, &call);
    let script = "import os,fcntl,sys;f=os.open(sys.argv[1],os.O_RDWR);fcntl.flock(f,fcntl.LOCK_EX);os.close(f)";
    let launch = |plan: &SandboxPlan, script: &str, path: &Path| {
        let (program, arguments) = plan.wrap(
            &runtime.python,
            [
                OsString::from("-c"),
                script.into(),
                path.as_os_str().to_owned(),
            ],
        );
        let mut process = crate::background::tokio_command(program);
        process
            .args(arguments)
            .envs(runtime.environment("narrate").unwrap());
        process
    };
    assert!(!launch(&plan, script, &lock)
        .output()
        .await
        .unwrap()
        .status
        .success());
    let granted = plan.with_host_writable_file(&lock);
    let allowed = launch(&granted, script, &lock).output().await.unwrap();
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let denied = launch(
        &granted,
        "import sys;open(sys.argv[1],'wb').write(b'outside')",
        &directory.path().join("unrelated-file"),
    )
    .output()
    .await
    .unwrap();
    assert!(!denied.status.success());
    assert!(!directory.path().join("unrelated-file").exists());
}

#[tokio::test]
#[ignore = "Installs private audio/video runtimes and performs real offline inference/rendering"]
async fn managed_audiovisual_end_to_end() {
    let home = std::env::var_os("JARVIS_TEST_AUDIOVISUAL_HOME")
        .expect("Set JARVIS_TEST_AUDIOVISUAL_HOME to an isolated test home");
    let home = Path::new(&home);
    assert!(home.is_absolute());
    fs::create_dir_all(home).unwrap();
    let current = crate::core::audiovisual::runtime(home).is_ok_and(|runtime| {
        fs::read_to_string(runtime.entry)
            .is_ok_and(|runner| runner == include_str!("../../core/audiovisual/runner.py"))
    });
    if !current {
        crate::core::audiovisual::tests::install_smoke_component(
            home,
            crate::core::ComponentId::Audiovisual,
        )
        .await;
    }
    let audio = crate::core::audiovisual::runtime(home).unwrap();
    assert_eq!(
        fs::read_to_string(&audio.entry).unwrap(),
        include_str!("../../core/audiovisual/runner.py"),
        "The real installer must publish the CURRENT embedded runner"
    );
    if crate::core::hyperframes::runtime(home).is_err() {
        crate::core::audiovisual::tests::install_smoke_component(
            home,
            crate::core::ComponentId::Hyperframes,
        )
        .await;
    }
    let video = crate::core::hyperframes::runtime(home).unwrap();
    for (topic, reference, expected) in [
        ("core", "references/data-attributes.md", "data-duration"),
        ("audio", "references/attributes.md", "data-automation"),
    ] {
        let document: Value =
            serde_json::from_str(&docs(home, &json!({"topic":topic,"file":reference})).unwrap())
                .unwrap();
        assert!(document["content"].as_str().unwrap().contains(expected));
    }
    let fixture = super::super::tests::Fixture::new();
    #[cfg(target_os = "macos")]
    verify_host_lock_scope(&fixture.root, home).await;
    let (_stop, signal) = watch::channel(false);
    let mut native = NativeTools {
        home,
        session: super::super::tests::session(&fixture),
        jobs: Jobs::default(),
        commands: CommandSessions::default(),
        signal,
    };
    native
        .execute(
            "video_run",
            json!({"action":"init","path":"smoke","yieldTimeMs":1000}),
        )
        .await;
    let directory = fixture.root.join("smoke");
    assert!(directory.join("assets/vendor/gsap.min.js").is_file());
    let narration = "Olá. Este vídeo tem voz e música.";
    let mut manifest = json!({"version":1,"voice":{"language":"pt-BR","voice":"pm_alex","speed":1},"scenes":[{"id":"intro","duration":1,"tail":0.35,"narration":narration,"visual":"Apresentação do Jarvis"}]});
    fs::write(directory.join("presentation.json"), manifest.to_string()).unwrap();
    let missing = native
        .execute("video_presentation", json!({"path":"smoke"}))
        .await;
    assert_eq!(missing["ready"], false);
    assert_eq!(missing["missing"].as_array().unwrap().len(), 1);
    let generated = native
        .execute("video_audio", missing["missing"][0]["arguments"].clone())
        .await;
    assert_eq!(generated["path"], "smoke/assets/audio/voice/intro.wav");
    let voice = directory.join("assets/audio/voice/intro.wav");
    assert!(audio::wave(&voice).unwrap().duration > 0.1);
    assert!(audio::valid_receipt(
        &serde_json::from_str(&fs::read_to_string(audio::receipt_path(&voice)).unwrap()).unwrap(),
        &voice,
    ));
    let confirmed = fs::read(&voice).unwrap();
    let reused = native
        .execute("video_audio", missing["missing"][0]["arguments"].clone())
        .await;
    assert_eq!(reused["reused"], true);
    assert!(reused.get("sessionId").is_none());
    assert_eq!(fs::read(&voice).unwrap(), confirmed);
    let timeline = native
        .execute("video_presentation", json!({"path":"smoke"}))
        .await;
    assert_eq!(timeline["ready"], true);
    let duration = timeline["duration"].as_f64().unwrap();
    native.execute("video_audio", json!({"action":"music","prompt":"gentle instrumental synth melody, no vocals","duration":duration,"seed":7,"nonCommercial":true,"output":"smoke/assets/audio/music/soundtrack.wav","yieldTimeMs":1000})).await;
    manifest["music"] = json!({"path":"assets/audio/music/soundtrack.wav","volume":0.16});
    fs::write(directory.join("presentation.json"), manifest.to_string()).unwrap();
    let timeline = native
        .execute("video_presentation", json!({"path":"smoke"}))
        .await;
    assert_eq!(timeline["ready"], true);
    assert_eq!(timeline["duration"].as_f64(), Some(duration));
    let audio_html = timeline["audioHtml"].as_str().unwrap();
    let captions = timeline["captionHtml"].as_str().unwrap();
    fs::write(directory.join("index.html"), format!(r##"<!doctype html><html lang="pt-BR"><head><meta charset="UTF-8"><script src="assets/vendor/gsap.min.js"></script><style>*{{box-sizing:border-box}}html,body{{margin:0;width:320px;height:180px;overflow:hidden;background:#181b20}}#root{{position:relative;width:100%;height:100%;font-family:sans-serif;color:#d7dce5}}#intro{{position:absolute;inset:0;display:flex;align-items:center;justify-content:center}}#title{{font-size:26px}}.presentation-caption{{position:absolute;bottom:18px;left:14px;right:14px;text-align:center;font-size:14px;line-height:1.4}}</style></head><body><main id="root" data-composition-id="jarvis-smoke" data-width="320" data-height="180" data-start="0" data-duration="{duration:.6}"><div id="intro" class="clip" data-start="0" data-duration="{duration:.6}" data-track-index="0"><span id="title">Jarvis · voz e música</span></div>{audio_html}{captions}</main><script>window.__timelines=window.__timelines||{{}};const motion=gsap.timeline({{paused:true}});motion.to("#title",{{x:12,duration:{duration:.6},ease:"none"}},0);window.__timelines["jarvis-smoke"]=motion;motion.seek(0);</script></body></html>"##)).unwrap();
    presentation::validate_html(&directory, &timeline).unwrap();
    native
        .execute(
            "video_run",
            json!({"action":"check","path":"smoke","yieldTimeMs":1000}),
        )
        .await;
    let rendered = native.execute("video_run", json!({"action":"render","path":"smoke","quality":"draft","output":"smoke/ready.mp4","yieldTimeMs":1000})).await;
    assert_eq!(rendered["path"], "smoke/ready.mp4");
    verify_video(&fixture.root, "smoke/ready.mp4").unwrap();
    let output = directory.join("ready.mp4");
    let probe = crate::background::tokio_command(&video.environment["HYPERFRAMES_FFPROBE_PATH"])
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .await
        .unwrap();
    assert!(
        probe.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = probe["streams"].as_array().unwrap();
    assert!(streams.iter().any(|stream| stream["codec_type"] == "video"));
    assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
    let actual = probe["format"]["duration"]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    assert!((actual - duration).abs() < 0.2, "{probe}");
    let decoded = crate::background::tokio_command(&video.environment["HYPERFRAMES_FFMPEG_PATH"])
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args([
            "-map",
            "0:a:0",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-ac",
            "1",
            "-ar",
            "16000",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    let samples = decoded
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f64 / i16::MAX as f64)
        .collect::<Vec<_>>();
    assert!(!samples.is_empty());
    let rms =
        (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt();
    assert!(rms > 0.001, "Rendered soundtrack is silent (RMS {rms})");
    assert!(native.commands.running_ids().is_empty());
    let artifact = std::env::temp_dir().join("jarvis-audiovisual-smoke.mp4");
    fs::copy(output, &artifact).unwrap();
    eprintln!(
        "AUDIOVISUAL_MP4={} duration={actual:.3}s RMS={rms:.6}",
        artifact.display()
    );
}
