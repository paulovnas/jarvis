//! Derive the shared audiovisual timeline from the editable project manifest.
use super::*;
use std::collections::HashSet;

pub(super) fn definition() -> Value {
    super::super::tools::definition("video_presentation", "Inspect a project composition's editable presentation.json and derive one scene timeline from actual PCM WAV narration durations. Read-only; returns missing/stale narration jobs, scene start/duration, duration, audioHtml (including music ducking) and captionHtml for native file tools to insert in index.html. Reuse valid scenes; regenerate only changed ones using a NEW audio path. No fixed inference timeout. Default narration assets/audio/voice/<id>.wav; explicitly set scene.audio to use a supplied recording. music.path is a local PCM WAV soundtrack with optional gain (default .16); generated MusicGen requires user-confirmed noncommercial use. Use the returned timings for visuals, narration and captions, then validate/render with video_run. This is the shared timing authority; do not independently guess durations.", json!({"path":{"type":"string","minLength":1,"maxLength":4096}}), &["path"])
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
}
fn source(
    root: &Path,
    directory: &Path,
    relative: &str,
    write: bool,
) -> Result<PathBuf, AgentError> {
    // Composition-local media also remains inside this agent's project.
    if write {
        if relative.is_empty()
            || relative.len() > 4096
            || relative.contains(['\0', '\\', ':'])
            || !Path::new(relative)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(error(
                "Use um caminho relativo dentro da composição, sem '..' ou links.",
            ));
        }
        let mut candidate = directory.to_path_buf();
        for part in Path::new(relative).components() {
            candidate.push(part);
            if fs::symlink_metadata(&candidate).is_ok() {
                super::super::tools::scoped(root, &candidate.to_string_lossy(), false)?;
            }
        }
        return Ok(candidate);
    }
    super::super::tools::scoped(directory, relative, write)?;
    let relative = directory
        .strip_prefix(root)
        .map_err(|_| AgentError::internal())?
        .join(relative);
    super::super::tools::scoped(root, &relative.to_string_lossy(), write)
}

pub(super) fn inspect(root: &Path, args: &Value) -> Result<Value, AgentError> {
    let path = args["path"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error("Informe a pasta da apresentação."))?;
    let directory = super::super::tools::scoped(root, path, false)?;
    if !directory.is_dir() {
        return Err(error(
            "A apresentação precisa estar em uma pasta do projeto.",
        ));
    }
    let manifest = source(root, &directory, "presentation.json", false)?;
    let value: Value = serde_json::from_str(&super::super::tools::read_text(&manifest)?)
        .map_err(|_| error("presentation.json precisa conter JSON válido."))?;
    let scenes = value["scenes"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .ok_or_else(|| error("Defina entre 1 e 64 cenas em presentation.json."))?;
    let voice = value["voice"]["voice"].as_str().unwrap_or("pm_alex");
    if !matches!(voice, "pf_dora" | "pm_alex" | "pm_santa") {
        return Err(error(
            "Escolha pf_dora, pm_alex ou pm_santa para a voz PT-BR.",
        ));
    }
    if value["voice"]["language"]
        .as_str()
        .is_some_and(|v| v != "pt-BR")
    {
        return Err(error("A narração nativa usa PT-BR."));
    }
    let speed = value["voice"]["speed"].as_f64().unwrap_or(1.0);
    if !(0.5..=2.0).contains(&speed) {
        return Err(error("Velocidade da voz inválida."));
    }
    let mut seen = HashSet::new();
    let mut start = 0.0;
    let mut timeline = Vec::new();
    let mut missing = Vec::new();
    let mut audio_html = String::new();
    let mut captions = String::new();
    let mut narration_ranges = Vec::new();
    for scene in scenes {
        let id = scene["id"]
            .as_str()
            .filter(|id| identifier(id))
            .ok_or_else(|| {
                error("Cada cena precisa de um id curto, com letras, números, hífen ou underscore.")
            })?;
        if !seen.insert(id) {
            return Err(error("Os ids das cenas não podem se repetir."));
        }
        let narration = scene["narration"].as_str().unwrap_or("").trim();
        if narration.len() > 4000 {
            return Err(error("Separe a narração extensa em cenas menores."));
        }
        let minimum = scene["duration"].as_f64().unwrap_or(1.0);
        let tail = scene["tail"].as_f64().unwrap_or(0.35);
        if !minimum.is_finite()
            || !(0.1..=3600.0).contains(&minimum)
            || !tail.is_finite()
            || !(0.0..=10.0).contains(&tail)
        {
            return Err(error("Duração ou pausa da cena inválida."));
        }
        let mut duration = minimum;
        let mut audio_path = None;
        let mut audio_duration = None;
        if !narration.is_empty() || scene["audio"].is_string() {
            let default = format!("assets/audio/voice/{id}.wav");
            let relative = scene["audio"].as_str().unwrap_or(&default);
            let candidate = source(root, &directory, relative, true)?;
            let mut valid = false;
            if candidate.exists() {
                let candidate = source(root, &directory, relative, false)?;
                let info = audio::wave(&candidate)?;
                // Explicit recordings need no Jarvis inference receipt. Default generated
                // narration is only reusable when its text, voice and speed still match.
                let record = audio::receipt_path(&candidate);
                let has_receipt = fs::symlink_metadata(&record).is_ok();
                let record = record
                    .strip_prefix(&directory)
                    .map_err(|_| AgentError::internal())?;
                let record = source(root, &directory, &record.to_string_lossy(), false)
                    .ok()
                    .and_then(|path| super::super::tools::read_text(&path).ok())
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok());
                let matches = record.as_ref().is_some_and(|v| {
                    audio::valid_receipt(v, &candidate)
                        && v["request"]["text"] == narration
                        && v["request"]["voice"] == voice
                        && v["request"]["speed"].as_f64() == Some(speed)
                });
                valid = if has_receipt {
                    matches
                } else {
                    scene["audio"].is_string()
                };
                if valid {
                    duration = duration.max(info.duration + tail);
                    audio_duration = Some(info.duration);
                    audio_path = Some(relative.to_owned());
                    audio_html.push_str(&format!("<audio id=\"narration-{id}\" class=\"clip\" src=\"{}\" data-start=\"{start:.6}\" data-duration=\"{:.6}\" data-track-index=\"10\" data-volume=\"1\"></audio>\n",escape(relative),info.duration));
                    narration_ranges.push((start, info.duration));
                }
            }
            if !valid {
                let output = directory
                    .strip_prefix(root)
                    .map_err(|_| AgentError::internal())?
                    .join(relative)
                    .to_string_lossy()
                    .replace('\\', "/");
                missing.push(json!({"scene":id,"reason":if candidate.exists(){"narration_changed"}else{"missing_audio"},"tool":"video_audio","arguments":{"action":"narrate","text":narration,"voice":voice,"speed":speed,"output":output},"existingPreserved":candidate.exists()}));
            }
        }
        if !narration.is_empty() {
            captions.push_str(&format!("<div id=\"caption-{id}\" class=\"clip presentation-caption\" data-start=\"{start:.6}\" data-duration=\"{duration:.6}\" data-track-index=\"20\">{}</div>\n",escape(narration)));
        }
        timeline.push(json!({"id":id,"start":start,"duration":duration,"narration":narration,"audio":audio_path,"audioDuration":audio_duration,"visual":scene["visual"]}));
        start += duration;
    }
    if let Some(relative) = value["music"]["path"].as_str() {
        let music = source(root, &directory, relative, false)?;
        let info = audio::wave(&music)?;
        if info.duration + 0.02 < start {
            return Err(error("A trilha é mais curta que a timeline. Gere uma duração adequada ou use uma trilha licenciada mais longa."));
        }
        let volume = value["music"]["volume"].as_f64().unwrap_or(0.16);
        if !(0.0..=1.0).contains(&volume) {
            return Err(error("Volume musical inválido."));
        }
        let points = music_envelope(start, volume, &narration_ranges);
        let automation = json!({"version":1,"lanes":[{"target":"volume","points":points}]});
        audio_html.push_str(&format!("<audio id=\"music-bed\" class=\"clip\" src=\"{}\" data-start=\"0\" data-duration=\"{start:.6}\" data-track-index=\"11\" data-volume=\"{volume}\" data-automation=\"{}\"></audio>\n",escape(relative),escape(&automation.to_string())));
    } else if value["music"]["prompt"]
        .as_str()
        .is_some_and(|v| !v.trim().is_empty())
    {
        missing.push(json!({"reason":"missing_soundtrack","tool":"video_audio","arguments":{"action":"music","prompt":value["music"]["prompt"],"duration":start,"nonCommercial":value["music"]["nonCommercial"],"output":format!("{path}/assets/audio/music/soundtrack.wav")},"instructions":"After narration is ready, generate music only with user-confirmed noncommercial use, or supply a licensed PCM WAV. Set music.path to its composition-local path and inspect again."}));
    }
    Ok(
        json!({"manifest":format!("{path}/presentation.json"),"ready":missing.is_empty(),"duration":start,"scenes":timeline,"missing":missing,"audioHtml":audio_html,"captionHtml":captions,"music":value["music"],"instructions":"Use these scene timings in index.html and GSAP. Insert audioHtml inside the composition root once; captions are optional and must be styled. This tool does not edit source. Regenerate only missing/changed scenes, preserve previous assets, then inspect again before check/render."}),
    )
}

fn music_envelope(duration: f64, volume: f64, voices: &[(f64, f64)]) -> Vec<Value> {
    let mut times = vec![
        0.0,
        0.25_f64.min(duration),
        (duration - 0.4).max(0.0),
        duration,
    ];
    for &(start, length) in voices {
        times.extend([
            (start - 0.12).max(0.0),
            start,
            start + length,
            (start + length + 0.25).min(duration),
        ]);
    }
    times.sort_by(f64::total_cmp);
    times.dedup();
    times
        .into_iter()
        .map(|t| {
            let duck = voices
                .iter()
                .map(|&(start, length)| {
                    let end = start + length;
                    if t < start - 0.12 || t > end + 0.25 {
                        1.0
                    } else if t < start {
                        1.0 - 0.65 * ((t - (start - 0.12)) / 0.12)
                    } else if t <= end {
                        0.35
                    } else {
                        0.35 + 0.65 * ((t - end) / 0.25)
                    }
                })
                .fold(1.0_f64, f64::min);
            let fade = (t / 0.25).min((duration - t) / 0.4).clamp(0.0, 1.0);
            json!({"t":t,"v":volume*duck*fade})
        })
        .collect()
}

pub(super) fn validate_html(directory: &Path, report: &Value) -> Result<(), AgentError> {
    use std::collections::BTreeMap;
    let path = super::super::tools::scoped(directory, "index.html", false)?;
    let html = super::super::tools::read_text(&path)?;
    // Check the declared HyperFrames contract, not arbitrary browser DOM.
    // Comments/scripts cannot satisfy the audio delivery requirement.
    let stripped = regex::Regex::new(
        r"(?is)<!--.*?-->|<script\b[^>]*>.*?</script\s*>|<style\b[^>]*>.*?</style\s*>",
    )
    .unwrap()
    .replace_all(&html, "");
    let tag = regex::Regex::new(r"(?is)<([a-z][a-z0-9:_-]*)\b([^<>]*)>").unwrap();
    let attribute =
        regex::Regex::new(r#"([a-zA-Z][\w:-]*)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    let tags = tag
        .captures_iter(&stripped)
        .map(|captures| {
            let attrs = attribute
                .captures_iter(&captures[2])
                .filter_map(|item| {
                    let value = item.get(2).or_else(|| item.get(3))?.as_str();
                    quick_xml::escape::unescape(value)
                        .ok()
                        .map(|value| (item[1].to_ascii_lowercase(), value.into_owned()))
                })
                .collect::<BTreeMap<_, _>>();
            (captures[1].to_ascii_lowercase(), attrs)
        })
        .collect::<Vec<_>>();
    let equal = |attrs: &BTreeMap<String, String>, name: &str, value: f64| {
        attrs
            .get(name)
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|v| (v - value).abs() < 0.001)
    };
    let duration = report["duration"]
        .as_f64()
        .ok_or_else(AgentError::internal)?;
    if !tags.iter().any(|(_, attrs)| {
        attrs.contains_key("data-composition-id") && equal(attrs, "data-duration", duration)
    }) {
        return Err(error("A duração de index.html diverge de presentation.json. Use a timeline real retornada por video_presentation antes de renderizar."));
    }
    for scene in report["scenes"]
        .as_array()
        .ok_or_else(AgentError::internal)?
    {
        let id = scene["id"].as_str().unwrap();
        let start = scene["start"].as_f64().unwrap();
        let length = scene["duration"].as_f64().unwrap();
        if !tags.iter().any(|(_, attrs)| {
            attrs.get("id").is_some_and(|v| v == id)
                && equal(attrs, "data-start", start)
                && equal(attrs, "data-duration", length)
        }) {
            return Err(error(&format!("A cena {id} em index.html não corresponde à timeline de video_presentation. Ajuste o início e a duração da cena.")));
        }
        if let Some(source) = scene["audio"].as_str() {
            let length = scene["audioDuration"].as_f64().unwrap();
            let id = format!("narration-{id}");
            if !tags.iter().any(|(tag, attrs)| {
                tag == "audio"
                    && attrs.get("id") == Some(&id)
                    && attrs.get("src").is_some_and(|v| v == source)
                    && equal(attrs, "data-start", start)
                    && equal(attrs, "data-duration", length)
            }) {
                return Err(error("A narração não está inserida com a timeline correta em index.html. Insira audioHtml de video_presentation antes de renderizar."));
            }
        }
    }
    if let Some(source) = report["music"]["path"].as_str() {
        if !tags.iter().any(|(tag, attrs)| {
            tag == "audio"
                && attrs.get("id").is_some_and(|id| id == "music-bed")
                && attrs.get("src").is_some_and(|v| v == source)
                && equal(attrs, "data-start", 0.0)
                && equal(attrs, "data-duration", duration)
        }) {
            return Err(error("A trilha não está inserida com a timeline correta em index.html. Insira audioHtml de video_presentation antes de renderizar."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_cannot_publish_a_silent_or_outdated_composition() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let report = json!({"duration":2.35,"scenes":[{"id":"intro","start":0,"duration":2.35,"audio":"voice.wav","audioDuration":2.0}]});
        let base="<main data-composition-id='presentation' data-duration='2.35'><section id='intro' data-start='0' data-duration='2.35'></section>";
        fs::write(root.join("index.html"), base).unwrap();
        assert!(validate_html(&root, &report).is_err());
        let track =
            "<audio id='narration-intro' src='voice.wav' data-start='0' data-duration='2'></audio>";
        fs::write(
            root.join("index.html"),
            format!("{base}<!-- {track} --></main>"),
        )
        .unwrap();
        assert!(validate_html(&root, &report).is_err());
        fs::write(root.join("index.html"), format!("{base}{track}</main>")).unwrap();
        assert!(validate_html(&root, &report).is_ok());
        fs::write(
            root.join("index.html"),
            format!("{base}{track}</main>").replace("data-duration='2.35'", "data-duration='1'"),
        )
        .unwrap();
        assert!(validate_html(&root, &report).is_err());
    }
    #[test]
    fn narration_samples_drive_shared_timeline_and_music_ducks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let root = root.as_path();
        audio::tests::write_wave(&root.join("voice.wav"), 2);
        audio::tests::write_wave(&root.join("music.wav"), 6);
        fs::write(root.join("presentation.json"),json!({"music":{"path":"music.wav"},"scenes":[{"id":"intro","duration":1,"narration":"Olá <mundo>","audio":"voice.wav"},{"id":"end","duration":1}]}).to_string()).unwrap();
        let report = inspect(root, &json!({"path":"."})).unwrap();
        assert_eq!(report["ready"], true);
        assert_eq!(report["scenes"][1]["start"], 2.35);
        assert_eq!(report["duration"], 3.35);
        assert!(report["audioHtml"]
            .as_str()
            .unwrap()
            .contains("data-automation"));
        assert!(report["captionHtml"]
            .as_str()
            .unwrap()
            .contains("&lt;mundo&gt;"));
    }
    #[test]
    fn preserves_manifest_reports_missing_scene_and_rejects_escape() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let root = root.as_path();
        let value = json!({"scenes":[{"id":"intro","narration":"Olá"}]});
        fs::write(root.join("presentation.json"), value.to_string()).unwrap();
        let report = inspect(root, &json!({"path":"."})).unwrap();
        assert_eq!(report["ready"], false);
        assert_eq!(report["missing"][0]["arguments"]["voice"], "pm_alex");
        assert_eq!(
            fs::read_to_string(root.join("presentation.json")).unwrap(),
            value.to_string()
        );
        fs::write(
            root.join("presentation.json"),
            json!({"scenes":[{"id":"intro","audio":"../escape.wav"}]}).to_string(),
        )
        .unwrap();
        assert!(inspect(root, &json!({"path":"."})).is_err());
        assert!(!root.join("assets").exists());
    }

    #[test]
    fn fade_in_never_releases_ducking_during_speech_and_points_are_unique() {
        let points = music_envelope(4.0, 0.16, &[(0.0, 2.0), (2.35, 1.0)]);
        let times = points
            .iter()
            .map(|v| v["t"].as_f64().unwrap())
            .collect::<Vec<_>>();
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
        let early = points
            .iter()
            .find(|v| v["t"].as_f64() == Some(0.25))
            .unwrap()["v"]
            .as_f64()
            .unwrap();
        assert!((early - 0.16 * 0.35).abs() < 1e-9);
    }

    #[test]
    fn altered_generated_audio_is_not_silently_reused() {
        use sha2::{Digest, Sha256};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = root.join("voice.wav");
        audio::tests::write_wave(&path, 2);
        let request = json!({"action":"narrate","text":"Olá","voice":"pm_alex","speed":1.0});
        let receipt = json!({"version":1,"request":request,"requestHash":format!("{:x}",Sha256::digest(request.to_string().as_bytes())),"audioHash":format!("{:x}",Sha256::digest(fs::read(&path).unwrap()))});
        fs::write(audio::receipt_path(&path), receipt.to_string()).unwrap();
        fs::write(
            root.join("presentation.json"),
            json!({"scenes":[{"id":"intro","narration":"Olá","audio":"voice.wav"}]}).to_string(),
        )
        .unwrap();
        assert_eq!(inspect(&root, &json!({"path":"."})).unwrap()["ready"], true);
        let mut bytes = fs::read(&path).unwrap();
        bytes[44] = 1;
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            inspect(&root, &json!({"path":"."})).unwrap()["ready"],
            false
        );
    }
}
