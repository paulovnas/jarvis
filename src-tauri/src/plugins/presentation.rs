//! Bounded local presentation assets; never executable plugin configuration.
use super::{manifest, AvailablePlugin, PackageSource, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use quick_xml::{events::Event, Reader};
use serde_json::Value;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const MAX_ICON_BYTES: u64 = 256 * 1024;

#[derive(Default)]
pub(super) struct Metadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub short_description: Option<String>,
    pub category: Option<String>,
    pub version: Option<String>,
    pub icons: Vec<String>,
}

fn text(value: &Value, key: &str, limit: usize) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= limit
                && !value.chars().any(|character| character == '\0')
        })
        .map(str::to_owned)
}

pub(super) fn fields(value: &Value) -> Metadata {
    let interface = value.get("interface").unwrap_or(value);
    Metadata {
        display_name: text(interface, "displayName", 256)
            .or_else(|| text(value, "displayName", 256)),
        description: text(interface, "longDescription", 16384)
            .or_else(|| text(value, "description", 16384)),
        short_description: text(interface, "shortDescription", 1024),
        category: text(interface, "category", 128).or_else(|| text(value, "category", 128)),
        version: text(value, "version", 128),
        icons: ["composerIcon", "logoDark", "logo"]
            .iter()
            .filter_map(|key| text(interface, key, 1024))
            .collect(),
    }
}

pub(super) fn load(root: &Path) -> Result<Metadata> {
    let (document, _) = manifest::plugin_document(root)?;
    Ok(fields(&document))
}

pub(super) fn enrich(plugin: &mut AvailablePlugin, entry: &Value, with_icon: bool) {
    let fallback = fields(entry);
    let local = if let PackageSource::Local { path } = &plugin.source {
        Some(PathBuf::from(path))
    } else {
        None
    };
    let loaded = local
        .as_deref()
        .and_then(|root| load(root).ok())
        .unwrap_or_default();
    if let Some(name) = loaded.display_name.or(fallback.display_name) {
        plugin.display_name = name;
    }
    if let Some(description) = loaded.description.or(fallback.description) {
        plugin.description = description;
    }
    plugin.short_description = loaded.short_description.or(fallback.short_description);
    // The marketplace owns taxonomy, as in Codex's receiver.
    plugin.category = text(entry, "category", 128)
        .or(fallback.category)
        .or(loaded.category);
    if let Some(version) = loaded.version.or(fallback.version) {
        plugin.version = Some(version);
    }
    plugin.icon_data_url = if with_icon {
        local.as_deref().and_then(|root| icon(root, &loaded.icons))
    } else {
        None
    };
    let document = local
        .as_deref()
        .and_then(|root| manifest::plugin_document(root).ok());
    let configuration = document.as_ref().map_or(entry, |(document, _)| document);
    for (key, message) in [
        (
            "lspServers",
            "Servidores LSP deste pacote não são ativados pelo Jarvis.",
        ),
        (
            "agents",
            "Agentes declarados neste pacote não são importados como agentes Jarvis.",
        ),
    ] {
        if configuration.get(key).is_some_and(nonempty)
            && !plugin.requirements.iter().any(|value| value == message)
        {
            plugin.requirements.push(message.into());
        }
    }
    let unsupported = ["lspServers", "agents"]
        .iter()
        .any(|key| configuration.get(key).is_some_and(nonempty));
    let conventional_components = local.as_deref().is_some_and(|root| {
        [
            "skills",
            "commands",
            ".mcp.json",
            "hooks/hooks.json",
            ".app.json",
        ]
        .iter()
        .any(|path| root.join(path).exists())
    });
    let declared_components = ["skills", "commands", "mcpServers", "hooks", "apps"]
        .iter()
        .any(|key| configuration.get(key).is_some_and(nonempty));
    if unsupported && !conventional_components && !declared_components {
        plugin.installable = false;
    }
    if local.as_deref().is_some_and(Path::is_dir) && document.is_none() {
        plugin.installable = false;
        let message = "Este pacote não possui um manifest compatível; seus componentes inline ainda não podem ser instalados.";
        if !plugin.requirements.iter().any(|value| value == message) {
            plugin.requirements.push(message.into());
        }
    }
}

fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

pub(super) fn icon(root: &Path, paths: &[String]) -> Option<String> {
    paths.iter().find_map(|path| image(root, path))
}

fn image(root: &Path, reference: &str) -> Option<String> {
    let path = root.join(manifest::relative(reference, true).ok()?);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ICON_BYTES {
        return None;
    }
    let canonical_root = fs::canonicalize(root).ok()?;
    let canonical_path = fs::canonicalize(&path).ok()?;
    if !canonical_path.starts_with(canonical_root) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .ok()?
        .take(MAX_ICON_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return None;
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        let svg = std::str::from_utf8(&bytes).ok()?;
        return safe_svg(svg)
            .then(|| format!("data:image/svg+xml;base64,{}", STANDARD.encode(bytes)));
    }
    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let format = reader.format()?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
            | image::ImageFormat::Bmp
    ) {
        return None;
    }
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return None;
    }
    let image = image::load_from_memory_with_format(&bytes, format)
        .ok()?
        .thumbnail(96, 96);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(output.into_inner())
    ))
}

fn local_references(value: &str) -> bool {
    let normalized: String = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.contains(['\\', '@'])
        || normalized.contains("/*")
        || ["javascript:", "data:", "http:", "https:", "file:", "://"]
            .iter()
            .any(|needle| normalized.contains(needle))
    {
        return false;
    }
    let mut remainder = normalized.as_str();
    while let Some(index) = remainder.find("url(") {
        remainder = &remainder[index + 4..];
        let Some(end) = remainder.find(')') else {
            return false;
        };
        let target = remainder[..end].trim_matches(['\'', '"']);
        if !target.starts_with('#')
            || !target[1..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
        {
            return false;
        }
        remainder = &remainder[end + 1..];
    }
    true
}

fn safe_svg(svg: &str) -> bool {
    let mut reader = Reader::from_str(svg);
    let mut count = 0usize;
    let mut depth = 0usize;
    let mut roots = 0usize;
    loop {
        count += 1;
        if count > 10000 {
            return false;
        }
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                let empty = matches!(&event, Event::Empty(_));
                let element = match event {
                    Event::Start(element) | Event::Empty(element) => element,
                    _ => unreachable!(),
                };
                let name = element.name();
                let name = name.as_ref();
                if !matches!(
                    name,
                    "svg"
                        | "g"
                        | "path"
                        | "rect"
                        | "circle"
                        | "ellipse"
                        | "line"
                        | "polyline"
                        | "polygon"
                        | "defs"
                        | "linearGradient"
                        | "radialGradient"
                        | "stop"
                        | "clipPath"
                        | "mask"
                        | "filter"
                        | "feGaussianBlur"
                        | "feOffset"
                        | "feFlood"
                        | "feComposite"
                        | "feBlend"
                        | "feColorMatrix"
                        | "feMerge"
                        | "feMergeNode"
                        | "feComponentTransfer"
                        | "feFuncR"
                        | "feFuncG"
                        | "feFuncB"
                        | "feFuncA"
                        | "feDropShadow"
                        | "use"
                        | "title"
                        | "desc"
                        | "text"
                        | "tspan"
                ) {
                    return false;
                }
                if depth == 0 {
                    if name != "svg" || roots > 0 {
                        return false;
                    }
                    roots += 1;
                }
                for attribute in element.attributes() {
                    let Ok(attribute) = attribute else {
                        return false;
                    };
                    let key = attribute.key.as_ref();
                    let Ok(value) = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    else {
                        return false;
                    };
                    if key == "xmlns" {
                        if value != "http://www.w3.org/2000/svg" {
                            return false;
                        }
                        continue;
                    }
                    if key == "xmlns:xlink" {
                        if value != "http://www.w3.org/1999/xlink" {
                            return false;
                        }
                        continue;
                    }
                    if !matches!(
                        key,
                        "id" | "class"
                            | "style"
                            | "width"
                            | "height"
                            | "viewBox"
                            | "fill"
                            | "stroke"
                            | "opacity"
                            | "fill-opacity"
                            | "stroke-opacity"
                            | "stroke-width"
                            | "fill-rule"
                            | "clip-rule"
                            | "stroke-linecap"
                            | "stroke-linejoin"
                            | "stroke-miterlimit"
                            | "stroke-dasharray"
                            | "stroke-dashoffset"
                            | "transform"
                            | "preserveAspectRatio"
                            | "x"
                            | "y"
                            | "x1"
                            | "x2"
                            | "y1"
                            | "y2"
                            | "cx"
                            | "cy"
                            | "r"
                            | "rx"
                            | "ry"
                            | "d"
                            | "points"
                            | "offset"
                            | "gradientUnits"
                            | "gradientTransform"
                            | "spreadMethod"
                            | "stop-color"
                            | "stop-opacity"
                            | "clipPathUnits"
                            | "maskUnits"
                            | "maskContentUnits"
                            | "filter"
                            | "clip-path"
                            | "mask"
                            | "filterUnits"
                            | "primitiveUnits"
                            | "in"
                            | "in2"
                            | "result"
                            | "stdDeviation"
                            | "dx"
                            | "dy"
                            | "flood-color"
                            | "flood-opacity"
                            | "operator"
                            | "values"
                            | "type"
                            | "mode"
                            | "k1"
                            | "k2"
                            | "k3"
                            | "k4"
                            | "slope"
                            | "intercept"
                            | "amplitude"
                            | "exponent"
                            | "tableValues"
                            | "color-interpolation-filters"
                            | "shape-rendering"
                            | "href"
                            | "xlink:href"
                            | "font-size"
                            | "font-family"
                            | "font-weight"
                            | "text-anchor"
                            | "dominant-baseline"
                    ) || !local_references(&value)
                    {
                        return false;
                    }
                    if matches!(key, "href" | "xlink:href") && !value.starts_with('#') {
                        return false;
                    }
                    if matches!(key, "width" | "height")
                        && value
                            .trim_end_matches(['p', 'x', '%'])
                            .parse::<f64>()
                            .is_ok_and(|number| !number.is_finite() || number.abs() > 2048.0)
                    {
                        return false;
                    }
                }
                if empty {
                    continue;
                }
                depth += 1;
                if depth > 64 {
                    return false;
                }
            }
            Ok(Event::End(_)) => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            Ok(Event::Eof) => return roots == 1 && depth == 0,
            Ok(
                Event::Decl(_)
                | Event::Text(_)
                | Event::Comment(_)
                | Event::CData(_)
                | Event::GeneralRef(_),
            ) => {}
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_allows_internal_clipping_and_rejects_active_or_external_content() {
        assert!(safe_svg(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><clipPath id="clip"><path d="M0 0h32v32H0z"/></clipPath></defs><path clip-path="url(#clip)" fill="#ff9100" d="M0 0h32v32H0z"/></svg>"##
        ));
        for unsafe_svg in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg onload="alert(1)"/>"#,
            r#"<svg><foreignObject/></svg>"#,
            r#"<svg><use href="https://example.com/image.svg"/></svg>"#,
            r#"<svg><path fill="url(https://example.com/paint)"/></svg>"#,
            r#"<svg><path style="fill:u\72l(https://example.com/paint)"/></svg>"#,
            r#"<!DOCTYPE svg [<!ENTITY secret SYSTEM "file:///secret">]><svg/>"#,
            r#"<?xml-stylesheet href="https://example.com/style.css"?><svg/>"#,
            r#"<svg width="3000"/>"#,
            r#"<svg/><svg/>"#,
        ] {
            assert!(!safe_svg(unsafe_svg), "Accepted unsafe SVG: {unsafe_svg}");
        }
    }

    #[test]
    fn local_icons_are_bounded_contained_and_rasterized_as_png() {
        let root = tempfile::tempdir().unwrap();
        let image = image::RgbaImage::new(24, 24);
        image.save(root.path().join("logo.png")).unwrap();
        assert!(icon(root.path(), &["./logo.png".into()])
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(icon(root.path(), &["../logo.png".into()]).is_none());
        assert!(icon(root.path(), &["https://example.com/logo.png".into()]).is_none());
        fs::write(
            root.path().join("large.svg"),
            vec![b'x'; MAX_ICON_BYTES as usize + 1],
        )
        .unwrap();
        assert!(icon(root.path(), &["./large.svg".into()]).is_none());
        image::RgbaImage::new(2049, 1)
            .save(root.path().join("wide.png"))
            .unwrap();
        assert!(icon(root.path(), &["./wide.png".into()]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn external_symlink_assets_and_manifest_directories_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("logo.svg"), "<svg/>").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("assets")).unwrap();
        assert!(icon(root.path(), &["./assets/logo.svg".into()]).is_none());
        fs::write(outside.path().join("plugin.json"), r#"{"name":"outside"}"#).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(".codex-plugin")).unwrap();
        assert!(load(root.path()).is_err());
    }
}
