use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::{json, Value};
use std::io::Cursor;

fn encoded(image: &RgbaImage) -> String {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    format!(
        "data:image/png;base64,{}",
        STANDARD.encode(bytes.into_inner())
    )
}

fn run(operation: &str, image: &RgbaImage, options: Value) -> Result<Response, String> {
    process(
        serde_json::from_value(
            json!({"operation":operation,"image":encoded(image),"options":options}),
        )
        .unwrap(),
    )
}

fn decoded(output: &Output) -> RgbaImage {
    let bytes = STANDARD
        .decode(
            output
                .data_url
                .strip_prefix("data:image/png;base64,")
                .unwrap(),
        )
        .unwrap();
    image::load_from_memory(&bytes).unwrap().into_rgba8()
}

fn checker(pitch: u32) -> RgbaImage {
    RgbaImage::from_fn(10 * pitch, 8 * pitch, |x, y| {
        if (x / pitch + y / pitch).is_multiple_of(2) {
            Rgba([220, 40, 30, 255])
        } else {
            Rgba([25, 80, 210, 255])
        }
    })
}

#[test]
fn pixel_processing_recovers_a_periodic_native_grid_without_target_size_guessing() {
    let result = run("recover_grid", &checker(4), json!({})).unwrap();
    assert_eq!((result.images[0].width, result.images[0].height), (10, 8));
    assert!((result.report["grid"]["cellWidth"].as_f64().unwrap() - 4.0).abs() < 0.01);
    assert!((result.report["grid"]["cellHeight"].as_f64().unwrap() - 4.0).abs() < 0.01);
    assert!(result.report["grid"]["confidence"].as_f64().unwrap() > 0.8);
    assert_eq!(
        decoded(&result.images[0]).get_pixel(0, 0),
        &Rgba([220, 40, 30, 255])
    );
}

#[test]
fn pixel_processing_detects_a_fractional_upscale_from_real_edge_spacing() {
    let image = image::imageops::resize(&checker(1), 107, 85, image::imageops::FilterType::Nearest);
    let result = run("recover_grid", &image, json!({})).unwrap();
    assert_eq!(result.images[0].width, 10);
    assert_eq!(result.images[0].height, 8);
    assert!((result.report["grid"]["cellWidth"].as_f64().unwrap() - 10.7).abs() < 0.2);
}

#[test]
fn pixel_processing_does_not_invent_residual_border_cells_after_fractional_fitting() {
    let native = RgbaImage::from_fn(5, 5, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba([220, 40, 30, 255])
        } else {
            Rgba([25, 80, 210, 255])
        }
    });
    let image = image::imageops::resize(&native, 49, 49, image::imageops::FilterType::Nearest);
    let result = run("recover_grid", &image, json!({})).unwrap();
    assert_eq!(result.images[0].width, 5);
    assert_eq!(result.images[0].height, 5);
    assert_eq!(decoded(&result.images[0]), native);
    let manual = run(
        "recover_grid",
        &image,
        json!({"cellWidth":9.8,"cellHeight":9.8}),
    )
    .unwrap();
    assert_eq!(decoded(&manual.images[0]), native);
}

#[test]
fn pixel_processing_preserves_a_subpixel_grid_phase_when_refining_periodicity() {
    let mut profile = vec![0.0; 180];
    for index in 0..17 {
        profile[(1.2 + index as f64 * 10.65).round() as usize] = 10.0;
    }
    let axis = detect_axis(&profile).unwrap();
    assert!(axis.confidence >= 0.95);
    assert!(axis.phase > 0.5 && axis.phase < 2.0);
}

#[test]
fn pixel_processing_preserves_alpha_and_uses_integer_upscaling() {
    let mut image = checker(4);
    for y in 0..4 {
        for x in 0..4 {
            image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        }
    }
    let before = encoded(&image);
    let result = run(
        "recover_grid",
        &image,
        json!({"cellWidth":4,"cellHeight":4,"scale":3}),
    )
    .unwrap();
    let native = decoded(&result.images[0]);
    let enlarged = decoded(&result.images[1]);
    assert_eq!(native.get_pixel(0, 0)[3], 0);
    assert_eq!(enlarged.dimensions(), (30, 24));
    for y in 0..24 {
        for x in 0..30 {
            assert_eq!(enlarged.get_pixel(x, y), native.get_pixel(x / 3, y / 3));
        }
    }
    assert_eq!(encoded(&image), before);
}

#[test]
fn pixel_processing_does_not_invent_a_grid_for_a_flat_image() {
    let image = RgbaImage::from_pixel(32, 32, Rgba([40, 80, 90, 255]));
    let error = run("recover_grid", &image, json!({})).unwrap_err();
    assert!(error.contains("cellWidth"));
    let result = run(
        "recover_grid",
        &image,
        json!({"cellWidth":4,"cellHeight":4}),
    )
    .unwrap();
    assert_eq!(result.images[0].width, 8);
}

#[test]
fn pixel_processing_rejects_invalid_settings_before_allocating_outputs() {
    for options in [
        json!({"cellWidth":0,"cellHeight":4}),
        json!({"cellWidth":4}),
        json!({"paletteColors":1}),
        json!({"scale":100}),
        json!({"phaseX":6,"cellWidth":4,"cellHeight":4}),
    ] {
        assert!(run("recover_grid", &checker(4), options).is_err());
    }
    assert!(run("extract_poses", &checker(4), json!({"columns":0})).is_err());
    assert!(serde_json::from_value::<Request>(
        json!({"operation":"recover_grid","image":"x","options":{"unsafe":true}})
    )
    .is_err());
}

fn poses() -> RgbaImage {
    let mut image = RgbaImage::new(40, 20);
    for y in 5..15 {
        for x in 4..17 {
            image.put_pixel(x, y, Rgba([210, 80, 30, 255]));
        }
    }
    // An attached weapon crosses the nominal 20-pixel cell boundary.
    for x in 16..24 {
        image.put_pixel(x, 10, Rgba([160, 160, 170, 255]));
    }
    for y in 5..15 {
        for x in 29..36 {
            image.put_pixel(x, y, Rgba([30, 100, 210, 255]));
        }
    }
    // Detached parts belong to their nearest body, not to the largest component only.
    image.put_pixel(3, 3, Rgba([220, 200, 50, 255]));
    image
}

#[test]
fn pixel_processing_extracts_crossing_poses_and_preserves_detached_parts() {
    let image = poses();
    let result = run("extract_poses", &image, json!({"columns":2,"rows":1})).unwrap();
    let frames: Vec<_> = result.images.iter().filter(|v| v.kind == "frame").collect();
    assert_eq!(frames.len(), 2);
    assert_eq!(result.report["poses"][0]["bounds"]["maxX"], 23);
    let count: usize = frames
        .iter()
        .map(|f| decoded(f).pixels().filter(|p| p[3] > 0).count())
        .sum();
    assert_eq!(count, image.pixels().filter(|p| p[3] > 0).count());
    assert!(decoded(frames[0])
        .pixels()
        .any(|p| p == &Rgba([220, 200, 50, 255])));
    assert!(!decoded(frames[0])
        .pixels()
        .any(|p| p == &Rgba([30, 100, 210, 255])));
    assert_eq!(frames[0].width, frames[1].width);
    assert_eq!(frames[0].height, frames[1].height);
}

#[test]
fn pixel_processing_assigns_a_crossing_weapon_by_body_pixels_instead_of_centroid() {
    let mut image = RgbaImage::new(40, 20);
    for y in 5..15 {
        for x in 16..20 {
            image.put_pixel(x, y, Rgba([210, 80, 30, 255]));
        }
    }
    for x in 20..35 {
        image.put_pixel(x, 10, Rgba([160, 160, 170, 255]));
    }
    for y in 5..15 {
        for x in 36..40 {
            image.put_pixel(x, y, Rgba([30, 100, 210, 255]));
        }
    }
    let result = run("extract_poses", &image, json!({"columns":2,"rows":1})).unwrap();
    assert_eq!(result.report["frames"], 2);
    assert_eq!(result.report["emptyCells"], json!([]));
    let frames: Vec<_> = result.images.iter().filter(|v| v.kind == "frame").collect();
    assert_eq!(decoded(frames[0]).pixels().filter(|p| p[3] > 0).count(), 55);
    assert_eq!(decoded(frames[1]).pixels().filter(|p| p[3] > 0).count(), 40);
}

#[test]
fn pixel_processing_keeps_detached_effects_with_explicit_occupied_cells() {
    let mut image = RgbaImage::new(60, 20);
    for y in 5..15 {
        for x in 4..15 {
            image.put_pixel(x, y, Rgba([210, 80, 30, 255]));
        }
    }
    for y in 5..15 {
        for x in 26..34 {
            image.put_pixel(x, y, Rgba([30, 100, 210, 255]));
        }
    }
    image.put_pixel(50, 3, Rgba([230, 220, 40, 255]));
    let result = run(
        "extract_poses",
        &image,
        json!({"columns":3,"rows":1,"poseCells":[0,1]}),
    )
    .unwrap();
    assert_eq!(result.report["frames"], 2);
    let count: usize = result
        .images
        .iter()
        .filter(|v| v.kind == "frame")
        .map(|v| decoded(v).pixels().filter(|p| p[3] > 0).count())
        .sum();
    assert_eq!(count, image.pixels().filter(|p| p[3] > 0).count());
    assert!(run(
        "extract_poses",
        &image,
        json!({"columns":2,"poseCells":[2]})
    )
    .is_err());
}

#[test]
fn pixel_processing_reports_empty_cells_and_keys_only_the_requested_background() {
    let mut image = RgbaImage::from_pixel(60, 20, Rgba([0, 255, 0, 255]));
    for y in 5..15 {
        for x in 4..15 {
            image.put_pixel(x, y, Rgba([255, 40, 60, 255]));
        }
    }
    for y in 5..15 {
        for x in 26..34 {
            image.put_pixel(x, y, Rgba([50, 70, 220, 255]));
        }
    }
    let result = run(
        "extract_poses",
        &image,
        json!({"columns":3,"rows":1,"chromaKey":[0,255,0]}),
    )
    .unwrap();
    assert_eq!(result.report["emptyCells"], json!([2]));
    assert_eq!(
        result.images.iter().filter(|i| i.kind == "frame").count(),
        2
    );
    assert!(!decoded(&result.images[0])
        .pixels()
        .any(|p| p == &Rgba([0, 255, 0, 255])));
    assert!(result.report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("vazia")));
}

#[test]
fn pixel_processing_preserves_vertical_motion_unless_bottom_alignment_is_explicit() {
    let mut image = RgbaImage::new(40, 20);
    for y in 10..18 {
        for x in 5..10 {
            image.put_pixel(x, y, Rgba([210, 90, 40, 255]));
        }
    }
    for y in 3..11 {
        for x in 25..30 {
            image.put_pixel(x, y, Rgba([210, 90, 40, 255]));
        }
    }
    let preserved = run("extract_poses", &image, json!({"columns":2,"rows":1})).unwrap();
    let aligned = run(
        "extract_poses",
        &image,
        json!({"columns":2,"rows":1,"alignment":"bottom"}),
    )
    .unwrap();
    let footlines = |result: &Response| {
        result
            .images
            .iter()
            .filter(|v| v.kind == "frame")
            .map(|v| {
                decoded(v)
                    .enumerate_pixels()
                    .filter(|(_, _, p)| p[3] > 0)
                    .map(|(_, y, _)| y)
                    .max()
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(footlines(&preserved)[0] - footlines(&preserved)[1], 7);
    assert_eq!(footlines(&aligned)[0], footlines(&aligned)[1]);
}

#[test]
fn pixel_processing_accepts_no_filesystem_or_remote_image_references() {
    for source in [
        "C:/private/image.png",
        "https://example.com/image.png",
        "data:image/png;base64,AAAA",
    ] {
        let request =
            serde_json::from_value(json!({"operation":"recover_grid","image":source,"options":{}}))
                .unwrap();
        assert!(process(request).is_err());
    }
}
