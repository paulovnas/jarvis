//! Bounded, deterministic image processing; no filesystem or network access.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor};

const MAX_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_OUTPUT_PIXELS: u64 = 24 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Request {
    RecoverGrid {
        image: String,
        #[serde(default)]
        options: GridOptions,
    },
    ExtractPoses {
        image: String,
        #[serde(default)]
        options: PoseOptions,
    },
}

fn palette_default() -> usize {
    256
}
fn alpha_default() -> u8 {
    16
}
fn one() -> u32 {
    1
}
fn padding_default() -> u32 {
    2
}
fn tolerance_default() -> u8 {
    30
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GridOptions {
    cell_width: Option<f64>,
    cell_height: Option<f64>,
    phase_x: f64,
    phase_y: f64,
    palette_colors: usize,
    scale: u32,
    alpha_threshold: u8,
    chroma_key: Option<[u8; 3]>,
    tolerance: u8,
}
impl Default for GridOptions {
    fn default() -> Self {
        Self {
            cell_width: None,
            cell_height: None,
            phase_x: 0.0,
            phase_y: 0.0,
            palette_colors: palette_default(),
            scale: 1,
            alpha_threshold: alpha_default(),
            chroma_key: None,
            tolerance: tolerance_default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Alignment {
    #[default]
    Preserve,
    Bottom,
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PoseOptions {
    columns: u32,
    rows: u32,
    padding: u32,
    min_area: usize,
    pose_cells: Option<Vec<usize>>,
    alpha_threshold: u8,
    chroma_key: Option<[u8; 3]>,
    tolerance: u8,
    alignment: Alignment,
}
impl Default for PoseOptions {
    fn default() -> Self {
        Self {
            columns: 1,
            rows: 1,
            padding: padding_default(),
            min_area: one() as usize,
            pose_cells: None,
            alpha_threshold: alpha_default(),
            chroma_key: None,
            tolerance: tolerance_default(),
            alignment: Alignment::Preserve,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Output {
    pub(crate) kind: String,
    name: String,
    pub(crate) data_url: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Response {
    pub(crate) images: Vec<Output>,
    pub(crate) report: Value,
}

pub(crate) fn worker_label(label: &str) -> bool {
    label
        .strip_prefix("pixel-worker-")
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()))
}

pub(crate) fn plugin() -> tauri::plugin::TauriPlugin<tauri::DynRuntime> {
    tauri::plugin::Builder::new("pixel-processing")
        .invoke_handler(tauri::generate_handler![process_pixel_art_image])
        .build()
}

#[tauri::command]
// Only the inlined plugin grants this command to the managed loopback worker.
pub(crate) async fn process_pixel_art_image(
    webview: tauri::Webview,
    request: Request,
) -> Result<Response, String> {
    if webview.label() != "main" && !worker_label(webview.label()) {
        return Err("O processamento está disponível somente na suíte de pixel art.".into());
    }
    tauri::async_runtime::spawn_blocking(move || process(request))
        .await
        .map_err(|_| {
            "Não foi possível concluir o processamento local. A imagem original foi preservada."
                .to_string()
        })?
}

fn decode(source: &str) -> Result<RgbaImage, String> {
    let encoded = source
        .strip_prefix("data:image/png;base64,")
        .filter(|s| s.len() <= 48 * 1024 * 1024)
        .ok_or("Envie os pixels da imagem como PNG; caminhos e URLs externos não são aceitos.")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "PNG inválido: codificação base64 incorreta.")?;
    let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits.clone());
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "PNG inválido ou dimensões acima do limite.")?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("A imagem deve conter até 8 megapixels e dimensões de até 8192 pixels.".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
    reader.limits(limits);
    reader
        .decode()
        .map(|i| i.into_rgba8())
        .map_err(|_| "Não foi possível decodificar o PNG.".into())
}

fn output(kind: &str, name: String, image: RgbaImage) -> Result<Output, String> {
    let (width, height) = image.dimensions();
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|_| "Não foi possível criar o PNG derivado.")?;
    Ok(Output {
        kind: kind.into(),
        name,
        data_url: format!(
            "data:image/png;base64,{}",
            STANDARD.encode(bytes.into_inner())
        ),
        width,
        height,
    })
}

pub(crate) fn process(request: Request) -> Result<Response, String> {
    let source = match &request {
        Request::RecoverGrid { image, .. } | Request::ExtractPoses { image, .. } => image,
    };
    let image = decode(source)?;
    let fingerprint = format!("{:x}", Sha256::digest(image.as_raw()));
    let mut result = match request {
        Request::RecoverGrid { options, .. } => recover_grid(image, options)?,
        Request::ExtractPoses { options, .. } => extract_poses(image, options)?,
    };
    result.report["sourceRgbaSha256"] = json!(fingerprint);
    Ok(result)
}

fn prepare(
    image: &mut RgbaImage,
    alpha: u8,
    key: Option<[u8; 3]>,
    tolerance: u8,
) -> Result<(), String> {
    if alpha == 0 || tolerance > 120 {
        return Err("Use alphaThreshold entre 1 e 255 e tolerance entre 0 e 120.".into());
    }
    for pixel in image.pixels_mut() {
        if pixel[3] < alpha
            || key.is_some_and(|key| (0..3).all(|c| pixel[c].abs_diff(key[c]) <= tolerance))
        {
            *pixel = Rgba([0, 0, 0, 0]);
        }
    }
    Ok(())
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}
fn bin(pixel: &Rgba<u8>) -> usize {
    ((pixel[0] as usize >> 3) << 10) | ((pixel[1] as usize >> 3) << 5) | (pixel[2] as usize >> 3)
}

// Bound training independently of image resolution; every input pixel still participates in recovery.
fn quantize(image: &RgbaImage, count: usize) -> (Vec<[u8; 3]>, Vec<usize>) {
    let mut histogram: BTreeMap<usize, ([u64; 3], u64)> = BTreeMap::new();
    for pixel in image.pixels().filter(|p| p[3] > 0) {
        let entry = histogram.entry(bin(pixel)).or_default();
        for c in 0..3 {
            entry.0[c] += u64::from(pixel[c]);
        }
        entry.1 += 1;
    }
    let mut colors: Vec<_> = histogram
        .iter()
        .map(|(id, (sum, n))| (*id, sum.map(|v| v as f64 / *n as f64), *n))
        .collect();
    colors.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let training = &colors[..colors.len().min(4096)];
    let k = count.min(training.len());
    let mut centers = Vec::new();
    if k > 0 {
        centers.push(training[0].1);
        let mut nearest = vec![f64::MAX; training.len()];
        for _ in 1..k {
            let previous = *centers.last().unwrap();
            let next = training
                .iter()
                .enumerate()
                .map(|(i, (_, color, weight))| {
                    nearest[i] = nearest[i].min(distance(*color, previous));
                    (i, nearest[i] * (*weight as f64).sqrt())
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|v| v.0)
                .unwrap();
            centers.push(training[next].1);
        }
        for _ in 0..4 {
            let mut sums = vec![[0.0; 3]; k];
            let mut weights = vec![0_u64; k];
            for (_, color, weight) in training {
                let cluster = centers
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        distance(*color, **a).total_cmp(&distance(*color, **b))
                    })
                    .unwrap()
                    .0;
                for c in 0..3 {
                    sums[cluster][c] += color[c] * (*weight as f64);
                }
                weights[cluster] += weight;
            }
            for i in 0..k {
                if weights[i] > 0 {
                    centers[i] = sums[i].map(|s| s / weights[i] as f64);
                }
            }
        }
    }
    let palette: Vec<_> = centers.iter().map(|c| c.map(|v| v.round() as u8)).collect();
    let mut lookup = vec![usize::MAX; 32768];
    for (id, color, _) in colors {
        lookup[id] = centers
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| distance(color, **a).total_cmp(&distance(color, **b)))
            .map_or(usize::MAX, |v| v.0);
    }
    let indices = image
        .pixels()
        .map(|p| {
            if p[3] == 0 {
                usize::MAX
            } else {
                lookup[bin(p)]
            }
        })
        .collect();
    (palette, indices)
}

fn edge_profiles(
    image: &RgbaImage,
    palette: &[[u8; 3]],
    indices: &[usize],
) -> (Vec<f64>, Vec<f64>) {
    let (w, h) = image.dimensions();
    let mut horizontal = vec![0.0; w as usize];
    let mut vertical = vec![0.0; h as usize];
    let difference = |a: usize, b: usize| {
        let alpha_a = image.as_raw()[a * 4 + 3];
        let alpha_b = image.as_raw()[b * 4 + 3];
        if alpha_a == 0 && alpha_b == 0 {
            return 0.0;
        }
        if indices[a] == usize::MAX || indices[b] == usize::MAX {
            return f64::from(alpha_a.abs_diff(alpha_b));
        }
        let a = palette[indices[a]];
        let b = palette[indices[b]];
        (0..3).map(|c| f64::from(a[c].abs_diff(b[c]))).sum::<f64>() / 3.0
    };
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            if x > 0 {
                horizontal[x] += difference(i, i - 1) / f64::from(h);
            }
            if y > 0 {
                vertical[y] += difference(i, i - w as usize) / f64::from(w);
            }
        }
    }
    (horizontal, vertical)
}

struct Axis {
    pitch: f64,
    phase: f64,
    confidence: f64,
}
fn detect_axis(profile: &[f64]) -> Result<Axis, String> {
    let max = profile.iter().copied().fold(0.0_f64, f64::max);
    let threshold = max * 0.15;
    let mut peaks = Vec::new();
    let mut i = 1;
    while i < profile.len() {
        if profile[i] <= threshold || profile[i] <= 0.0 {
            i += 1;
            continue;
        }
        let start = i;
        while i + 1 < profile.len() && profile[i + 1] > threshold {
            i += 1;
        }
        let peak = (start..=i)
            .max_by(|a, b| profile[*a].total_cmp(&profile[*b]))
            .unwrap();
        peaks.push((peak, profile[peak]));
        i += 1;
    }
    let total: f64 = peaks.iter().map(|p| p.1).sum();
    if peaks.len() < 4 || total == 0.0 {
        return Err("Não foi possível detectar uma grade periódica com segurança. Informe cellWidth e cellHeight para recuperação manual.".into());
    }
    let mut strongest = peaks.clone();
    strongest.sort_by(|a, b| b.1.total_cmp(&a.1));
    strongest.truncate(96);
    let sample_total: f64 = strongest.iter().map(|p| p.1).sum();
    let residual = |position: usize, pitch: f64, phase: f64| {
        let r = (position as f64 - phase).rem_euclid(pitch);
        r.min(pitch - r)
    };
    let mut best: Option<(f64, Axis)> = None;
    // Search fractional scales too: nearest-neighbor resizing can alternate 10- and 11px blocks.
    for eighths in 16..=((profile.len() / 3).min(128) * 8) {
        let pitch = eighths as f64 / 8.0;
        let tolerance = (pitch / 6.0).min(1.5);
        for half_phase in 0..(pitch * 2.0).ceil() as usize {
            let phase = half_phase as f64 / 2.0;
            let aligned: f64 = strongest
                .iter()
                .filter(|(p, _)| residual(*p, pitch, phase) <= tolerance)
                .map(|p| p.1)
                .sum();
            let confidence = aligned / sample_total;
            if confidence < 0.86 {
                continue;
            }
            let error: f64 = strongest
                .iter()
                .map(|(p, w)| w * (residual(*p, pitch, phase) / tolerance).min(1.0))
                .sum::<f64>()
                / sample_total;
            let score = confidence - error * 0.25 + pitch * 0.0001;
            if best.as_ref().is_none_or(|(old, _)| score > *old) {
                best = Some((
                    score,
                    Axis {
                        pitch,
                        phase,
                        confidence,
                    },
                ));
            }
        }
    }
    let Some((_, mut axis)) = best else {
        return Err("A grade não apresenta periodicidade suficiente. Informe cellWidth e cellHeight; a imagem original permanece intacta.".into());
    };
    let original = (axis.pitch, axis.phase);
    let confidence_for = |pitch: f64, phase: f64| {
        peaks
            .iter()
            .filter(|(p, _)| residual(*p, pitch, phase) <= (pitch / 6.0).min(1.5))
            .map(|p| p.1)
            .sum::<f64>()
            / total
    };
    let tolerance = (axis.pitch / 6.0).min(1.5);
    let aligned: Vec<_> = peaks
        .iter()
        .filter(|(p, _)| residual(*p, axis.pitch, axis.phase) <= tolerance)
        .collect();
    let weight: f64 = aligned.iter().map(|p| p.1).sum();
    let mean_k = aligned
        .iter()
        .map(|(p, w)| w * ((*p as f64 - axis.phase) / axis.pitch).round())
        .sum::<f64>()
        / weight;
    let mean_x = aligned.iter().map(|(p, w)| w * (*p as f64)).sum::<f64>() / weight;
    let covariance = aligned
        .iter()
        .map(|(p, w)| {
            w * (((*p as f64 - axis.phase) / axis.pitch).round() - mean_k) * (*p as f64 - mean_x)
        })
        .sum::<f64>();
    let variance = aligned
        .iter()
        .map(|(p, w)| w * (((*p as f64 - axis.phase) / axis.pitch).round() - mean_k).powi(2))
        .sum::<f64>();
    if variance > 0.0 {
        axis.pitch = (covariance / variance).clamp(2.0, 128.0);
        axis.phase = (mean_x - axis.pitch * mean_k).rem_euclid(axis.pitch);
    }
    if confidence_for(axis.pitch, axis.phase) < confidence_for(original.0, original.1) {
        axis.pitch = original.0;
        axis.phase = original.1;
    }
    axis.confidence = confidence_for(axis.pitch, axis.phase);
    if axis.confidence < 0.86 {
        return Err("A grade fracionária ficou ambígua. Informe cellWidth e cellHeight; o original foi preservado.".into());
    }
    Ok(axis)
}

fn cuts(length: u32, axis: &Axis, profile: &[f64], adjust: bool) -> Vec<u32> {
    let mut result = vec![0];
    let edge_threshold = profile.iter().copied().fold(0.0_f64, f64::max) * 0.15;
    let mut next = if axis.phase > 0.0 {
        axis.phase
    } else {
        axis.pitch
    };
    while next.round() < f64::from(length) {
        let expected = next.round() as u32;
        let cut = if adjust {
            let radius = (axis.pitch / 6.0).floor() as u32;
            let from = expected
                .saturating_sub(radius)
                .max(result.last().copied().unwrap() + 1);
            let to = (expected + radius).min(length - 1);
            let peak =
                (from..=to).max_by(|a, b| profile[*a as usize].total_cmp(&profile[*b as usize]));
            peak.filter(|p| profile[*p as usize] > 0.0)
                .unwrap_or(expected)
        } else {
            expected
        };
        let weak_border = adjust
            && f64::from(cut.min(length - cut)) < axis.pitch / 2.0
            && profile[cut as usize] <= edge_threshold;
        if !weak_border && cut > *result.last().unwrap() && cut < length {
            result.push(cut);
        }
        next += axis.pitch;
    }
    result.push(length);
    result
}

fn recover_grid(mut image: RgbaImage, options: GridOptions) -> Result<Response, String> {
    if !(2..=256).contains(&options.palette_colors) || !(1..=16).contains(&options.scale) {
        return Err("Use paletteColors entre 2 e 256 e scale inteiro entre 1 e 16.".into());
    }
    let manual=match (options.cell_width,options.cell_height) {
        (Some(w),Some(h)) if (1.0..=128.0).contains(&w) && (1.0..=128.0).contains(&h) && options.phase_x>=0.0 && options.phase_y>=0.0 && options.phase_x<w && options.phase_y<h => Some((w,h)),
        (None,None) if options.phase_x==0.0 && options.phase_y==0.0 => None,
        _=>return Err("Informe cellWidth e cellHeight entre 1 e 128; phaseX/phaseY devem ser menores que a célula.".into()),
    };
    prepare(
        &mut image,
        options.alpha_threshold,
        options.chroma_key,
        options.tolerance,
    )?;
    let (palette, indices) = quantize(&image, options.palette_colors);
    if palette.is_empty() {
        return Err("A imagem não contém pixels visíveis após a remoção do fundo.".into());
    }
    let (px, py) = edge_profiles(&image, &palette, &indices);
    let (x, y) = if let Some((w, h)) = manual {
        (
            Axis {
                pitch: w,
                phase: options.phase_x,
                confidence: 1.0,
            },
            Axis {
                pitch: h,
                phase: options.phase_y,
                confidence: 1.0,
            },
        )
    } else {
        (detect_axis(&px)?, detect_axis(&py)?)
    };
    let xs = cuts(image.width(), &x, &px, manual.is_none());
    let ys = cuts(image.height(), &y, &py, manual.is_none());
    let width = (xs.len() - 1) as u32;
    let height = (ys.len() - 1) as u32;
    if u64::from(width) * u64::from(height) * u64::from(options.scale).pow(2) > MAX_PIXELS {
        return Err("A ampliação excede 8 megapixels. Reduza scale.".into());
    }
    let mut native = RgbaImage::new(width, height);
    for (row, yy) in ys.windows(2).enumerate() {
        for (col, xx) in xs.windows(2).enumerate() {
            let mut votes = vec![0_u64; palette.len() + 1];
            let mut alpha = vec![0_u64; palette.len()];
            for sy in yy[0]..yy[1] {
                for sx in xx[0]..xx[1] {
                    let index = indices[(sy * image.width() + sx) as usize];
                    if index == usize::MAX {
                        votes[palette.len()] += 1;
                    } else {
                        votes[index] += 1;
                        alpha[index] += u64::from(image.get_pixel(sx, sy)[3]);
                    }
                }
            }
            let winner = votes
                .iter()
                .enumerate()
                .max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i)))
                .unwrap()
                .0;
            if winner < palette.len() {
                let c = palette[winner];
                native.put_pixel(
                    col as u32,
                    row as u32,
                    Rgba([c[0], c[1], c[2], (alpha[winner] / votes[winner]) as u8]),
                );
            }
        }
    }
    let mut images = vec![output("native", "grade-nativa.png".into(), native.clone())?];
    if options.scale > 1 {
        let enlarged = image::imageops::resize(
            &native,
            width * options.scale,
            height * options.scale,
            image::imageops::FilterType::Nearest,
        );
        images.push(output(
            "scaled",
            format!("grade-{}x.png", options.scale),
            enlarged,
        )?);
    }
    Ok(Response {
        images,
        report: json!({
            "version":"jarvis.pixel-grid.v1","source":{"width":image.width(),"height":image.height()},
            "grid":{"cellWidth":x.pitch,"cellHeight":y.pitch,"phaseX":x.phase,"phaseY":y.phase,"confidence":x.confidence.min(y.confidence),"mode":if manual.is_some() {"manual"} else {"detected"},"xCuts":xs,"yCuts":ys},
            "native":{"width":width,"height":height},"paletteColors":palette.len(),"scale":options.scale,
            "warnings":if manual.is_some() {vec!["Grade definida manualmente; a periodicidade não foi inferida."]} else {vec![]},
            "originalPreserved":true
        }),
    })
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}
impl Bounds {
    fn include(&mut self, x: u32, y: u32) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }
    fn width(self) -> u32 {
        self.max_x - self.min_x + 1
    }
    fn height(self) -> u32 {
        self.max_y - self.min_y + 1
    }
    fn center(self) -> (f64, f64) {
        (
            f64::from(self.min_x + self.max_x) / 2.0,
            f64::from(self.min_y + self.max_y) / 2.0,
        )
    }
}
struct Component {
    pixels: Vec<usize>,
    bounds: Bounds,
    center: (f64, f64),
}
fn components(image: &RgbaImage, min_area: usize) -> (Vec<Component>, usize) {
    let (w, h) = image.dimensions();
    let mut seen = vec![false; (w * h) as usize];
    let mut result = Vec::new();
    let mut discarded = 0;
    for start in 0..seen.len() {
        if seen[start] || image.as_raw()[start * 4 + 3] == 0 {
            continue;
        }
        seen[start] = true;
        let (x, y) = (start as u32 % w, start as u32 / w);
        let mut part = Component {
            pixels: vec![start],
            bounds: Bounds {
                min_x: x,
                min_y: y,
                max_x: x,
                max_y: y,
            },
            center: (0.0, 0.0),
        };
        let mut cursor = 0;
        while cursor < part.pixels.len() {
            let p = part.pixels[cursor];
            cursor += 1;
            let (x, y) = (p as u32 % w, p as u32 / w);
            part.bounds.include(x, y);
            part.center.0 += f64::from(x);
            part.center.1 += f64::from(y);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let index = (ny * w + nx) as usize;
                    if !seen[index] && image.as_raw()[index * 4 + 3] > 0 {
                        seen[index] = true;
                        part.pixels.push(index);
                    }
                }
            }
        }
        if part.pixels.len() < min_area {
            discarded += part.pixels.len();
            continue;
        }
        part.center.0 /= part.pixels.len() as f64;
        part.center.1 /= part.pixels.len() as f64;
        result.push(part);
    }
    (result, discarded)
}

fn extract_poses(mut image: RgbaImage, options: PoseOptions) -> Result<Response, String> {
    if options.columns == 0
        || options.rows == 0
        || options.columns > 32
        || options.rows > 32
        || options.columns * options.rows > 64
        || options.padding > 64
        || options.min_area == 0
        || options.min_area > 10000
        || options.columns > image.width()
        || options.rows > image.height()
    {
        return Err(
            "Use de 1 a 64 células no total, padding até 64 e minArea entre 1 e 10000.".into(),
        );
    }
    prepare(
        &mut image,
        options.alpha_threshold,
        options.chroma_key,
        options.tolerance,
    )?;
    let (parts, discarded) = components(&image, options.min_area);
    if parts.is_empty() {
        return Err(
            "Nenhuma pose visível foi encontrada. Confira a transparência, chromaKey e minArea."
                .into(),
        );
    }
    if parts.len() == 1
        && parts[0].pixels.len() as u64
            > u64::from(image.width()) * u64::from(image.height()) * 9 / 10
    {
        return Err("O fundo ainda faz parte de um único componente. Informe chromaKey ou use uma imagem com transparência.".into());
    }
    let slots = (options.columns * options.rows) as usize;
    if let Some(cells) = &options.pose_cells {
        if cells.is_empty()
            || cells.iter().any(|id| *id >= slots)
            || cells
                .iter()
                .enumerate()
                .any(|(i, id)| cells[..i].contains(id))
        {
            return Err(
                "poseCells deve listar células distintas entre zero e columns × rows − 1.".into(),
            );
        }
    }
    let mut seeds: Vec<Option<usize>> = vec![None; slots];
    for (index, part) in parts.iter().enumerate() {
        // Assign the body by occupied area; a long thin weapon must not move it to another slot.
        let mut overlap = vec![0usize; slots];
        for pixel in &part.pixels {
            let (x, y) = (*pixel as u32 % image.width(), *pixel as u32 / image.width());
            let col = ((x + 1) * options.columns - 1) / image.width();
            let row = ((y + 1) * options.rows - 1) / image.height();
            overlap[(row * options.columns + col) as usize] += 1;
        }
        let slot = (0..slots)
            .max_by_key(|id| (overlap[*id], std::cmp::Reverse(*id)))
            .unwrap();
        if options
            .pose_cells
            .as_ref()
            .is_some_and(|cells| !cells.contains(&slot))
        {
            continue;
        }
        if seeds[slot].is_none_or(|old| part.pixels.len() > parts[old].pixels.len()) {
            seeds[slot] = Some(index);
        }
    }
    if options
        .pose_cells
        .as_ref()
        .is_some_and(|cells| cells.iter().any(|id| seeds[*id].is_none()))
    {
        return Err("Uma célula indicada em poseCells não contém o corpo de uma pose. Confira a grade e as células ocupadas.".into());
    }
    let bodies: Vec<_> = seeds
        .iter()
        .enumerate()
        .filter_map(|(slot, seed)| seed.map(|id| (slot, id)))
        .collect();
    let mut groups: Vec<Vec<usize>> = vec![vec![]; bodies.len()];
    for (index, part) in parts.iter().enumerate() {
        let owner = bodies
            .iter()
            .position(|(_, seed)| *seed == index)
            .unwrap_or_else(|| {
                bodies
                    .iter()
                    .enumerate()
                    .min_by(|(_, (_, a)), (_, (_, b))| {
                        let metric = |id: usize| {
                            let c = parts[id].bounds.center();
                            (part.center.0 - c.0).powi(2) + (part.center.1 - c.1).powi(2)
                        };
                        metric(*a).total_cmp(&metric(*b))
                    })
                    .unwrap()
                    .0
            });
        groups[owner].push(index);
    }
    let mut bounds = Vec::new();
    for group in &groups {
        let mut b = parts[group[0]].bounds;
        for id in group {
            let v = parts[*id].bounds;
            b.include(v.min_x, v.min_y);
            b.include(v.max_x, v.max_y);
        }
        bounds.push(b);
    }
    let cell_width = image.width().div_ceil(options.columns);
    let cell_height = image.height().div_ceil(options.rows);
    let mut left = options.padding;
    let mut top = options.padding;
    let mut right = options.padding;
    let mut bottom = options.padding;
    for ((slot, _), b) in bodies.iter().zip(&bounds) {
        let x = (*slot as u32 % options.columns) * image.width() / options.columns;
        let y = (*slot as u32 / options.columns) * image.height() / options.rows;
        left = left.max(x.saturating_sub(b.min_x) + options.padding);
        top = top.max(y.saturating_sub(b.min_y) + options.padding);
        right = right.max((b.max_x + 1).saturating_sub(x + cell_width) + options.padding);
        bottom = bottom.max((b.max_y + 1).saturating_sub(y + cell_height) + options.padding);
    }
    let (width, height) = match options.alignment {
        Alignment::Preserve => (cell_width + left + right, cell_height + top + bottom),
        Alignment::Bottom => (
            bounds.iter().map(|b| b.width()).max().unwrap() + 2 * options.padding,
            bounds.iter().map(|b| b.height()).max().unwrap() + 2 * options.padding,
        ),
    };
    let columns = options.columns.min(groups.len() as u32);
    let rows = (groups.len() as u32).div_ceil(columns);
    if u64::from(width) * u64::from(height) * (groups.len() as u64 + u64::from(columns * rows))
        > MAX_OUTPUT_PIXELS
    {
        return Err(
            "As poses derivadas excedem 24 megapixels. Reduza a imagem ou extraia menos células."
                .into(),
        );
    }
    let mut atlas = RgbaImage::new(width * columns, height * rows);
    let mut frames = Vec::new();
    let mut poses = Vec::new();
    let mut warnings = Vec::new();
    for (index, ((slot, _), group)) in bodies.iter().zip(&groups).enumerate() {
        let b = bounds[index];
        let cell_x = (*slot as u32 % options.columns) * image.width() / options.columns;
        let cell_y = (*slot as u32 / options.columns) * image.height() / options.rows;
        let (dx, dy) = match options.alignment {
            Alignment::Preserve => (
                i64::from(left) - i64::from(cell_x),
                i64::from(top) - i64::from(cell_y),
            ),
            Alignment::Bottom => (
                i64::from((width - b.width()) / 2) - i64::from(b.min_x),
                i64::from(height - options.padding - b.height()) - i64::from(b.min_y),
            ),
        };
        let mut frame = RgbaImage::new(width, height);
        for id in group {
            for pixel in &parts[*id].pixels {
                let (x, y) = (*pixel as u32 % image.width(), *pixel as u32 / image.width());
                let (tx, ty) = (i64::from(x) + dx, i64::from(y) + dy);
                if tx < 0 || ty < 0 || tx >= i64::from(width) || ty >= i64::from(height) {
                    return Err(
                        "Uma pose excedeu o canvas calculado; nenhum recorte foi aplicado.".into(),
                    );
                }
                frame.put_pixel(tx as u32, ty as u32, *image.get_pixel(x, y));
            }
        }
        image::imageops::overlay(
            &mut atlas,
            &frame,
            i64::from(index as u32 % columns * width),
            i64::from(index as u32 / columns * height),
        );
        frames.push(output(
            "frame",
            format!("pose-{:02}.png", index + 1),
            frame,
        )?);
        let crossing = b.min_x < cell_x
            || b.max_x >= cell_x + cell_width
            || b.min_y < cell_y
            || b.max_y >= cell_y + cell_height;
        if crossing {
            warnings.push(format!(
                "Pose {} ultrapassa a célula nominal; os pixels completos foram preservados.",
                index + 1
            ));
        }
        poses.push(json!({"index":index,"cell":slot,"bounds":b,"components":group.len(),"offset":{"x":dx,"y":dy},"crossesCell":crossing}));
    }
    let empty: Vec<_> = seeds
        .iter()
        .enumerate()
        .filter(|(_, seed)| seed.is_none())
        .map(|(i, _)| i)
        .collect();
    if !empty.is_empty() {
        warnings.push(format!(
            "Há {} célula(s) vazia(s); elas não foram transformadas em quadros.",
            empty.len()
        ));
    }
    if discarded > 0 {
        warnings.push(format!(
            "{discarded} pixels foram removidos pelo minArea solicitado."
        ));
    }
    let mut images = vec![output("sheet", "poses-extraidas.png".into(), atlas)?];
    images.extend(frames);
    Ok(Response {
        images,
        report: json!({"version":"jarvis.pose-components.v1","source":{"width":image.width(),"height":image.height()},
        "columns":columns,"rows":rows,"frameWidth":width,"frameHeight":height,"frames":poses.len(),
        "alignment":match options.alignment {Alignment::Preserve=>"preserve",Alignment::Bottom=>"bottom"},
        "poses":poses,"anchor":{"x":if matches!(options.alignment,Alignment::Preserve) {left+cell_width/2} else {width/2},"y":if matches!(options.alignment,Alignment::Preserve) {top+cell_height-1} else {height-options.padding-1}},
        "poseCells":options.pose_cells,"grouping":"component-area-and-nearest-body","emptyCells":empty,"discardedPixels":discarded,"warnings":warnings,"originalPreserved":true}),
    })
}

#[cfg(test)]
mod tests;
