//! Read-only qualification of native preview pixels and cache-hit geometry.
use serde::Serialize;
use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Instant};

#[derive(Debug, PartialEq)]
struct Options {
    project: PathBuf,
    frames: Vec<u32>,
    dimension: u32,
    composition: Option<u64>,
}

fn parse(args: Vec<OsString>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let project = PathBuf::from(
        args.next()
            .ok_or("Preview benchmark requires PROJECT.lep")?,
    );
    let mut frames = None;
    let mut dimension = None;
    let mut composition = None;
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--frames") if frames.is_none() => {
                let value = args
                    .next()
                    .ok_or("--frames requires comma-separated frames")?;
                let value = value.to_str().ok_or("Invalid frame list")?;
                if value.len() > 176 {
                    return Err("Preview benchmark supports 1..16 frames".into());
                }
                let parsed = value
                    .split(',')
                    .map(str::parse::<u32>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "Invalid frame list")?;
                if parsed.is_empty() || parsed.len() > 16 {
                    return Err("Preview benchmark supports 1..16 frames".into());
                }
                frames = Some(parsed);
            }
            Some("--dimension") if dimension.is_none() => {
                dimension = Some(
                    args.next()
                        .and_then(|s| s.to_str().and_then(|s| s.parse::<u32>().ok()))
                        .filter(|value| (1..=8192).contains(value))
                        .ok_or("--dimension requires 1..8192")?,
                );
            }
            Some("--composition") if composition.is_none() => {
                composition = Some(
                    args.next()
                        .and_then(|s| s.to_str().and_then(|s| s.parse::<u64>().ok()))
                        .filter(|value| *value > 0)
                        .ok_or("--composition requires a positive composition ID")?,
                );
            }
            _ => return Err("Unknown or repeated preview benchmark option".into()),
        }
    }
    Ok(Options {
        project,
        frames: frames.ok_or("Use --frames 300,4360")?,
        dimension: dimension.unwrap_or(1280),
        composition,
    })
}

#[derive(Serialize)]
struct Measurement {
    frame: u32,
    width: u32,
    height: u32,
    render_milliseconds: f64,
    cached_milliseconds: f64,
    shared_pixels: bool,
    matching_geometry: bool,
    evaluated_geometry: bool,
    hardware: libre_effects_gpu_render::Status,
    video_decoder: crate::video_decoder::Status,
    stages: crate::gpu_render::Timings,
    operations: Vec<(&'static str, u64, f64)>,
    filter_cache_hits: u64,
    path_cache_hits: u64,
}

pub(crate) fn run(args: Vec<OsString>) -> Result<(), String> {
    let options = parse(args)?;
    let mut project = crate::project_io::read_project(&options.project)?;
    if let Some(composition) = options.composition {
        project.activate_composition(composition)?;
    }
    if options
        .frames
        .iter()
        .any(|frame| *frame >= project.composition().duration())
    {
        return Err("Benchmark frame is outside the composition".into());
    }
    let before = libre_effects_core::project_file::encode(&project, None)?;
    let renderer = crate::rendering::Renderer::new();
    let mut cache = crate::preview_cache::Cache::default();
    cache.configure(
        &project,
        options.dimension,
        0,
        128 * crate::preview_cache::MIB,
    );
    let mut measurements = Vec::new();
    for frame in options.frames {
        let started = Instant::now();
        let fresh =
            crate::preview_frame::render(&renderer, &project, frame, options.dimension, None)?;
        let render_milliseconds = started.elapsed().as_secs_f64() * 1000.0;
        let stages = crate::gpu_render::timings();
        let operations = resvg::render_profile();
        cache.insert(frame, fresh.pixels.clone());
        let cached = cache
            .get(frame)
            .ok_or("Rendered preview exceeds benchmark's 128 MiB cache budget")?;
        let started = Instant::now();
        let hit = crate::preview_frame::render(
            &renderer,
            &project,
            frame,
            options.dimension,
            Some(cached),
        )?;
        let cached_milliseconds = started.elapsed().as_secs_f64() * 1000.0;
        let request = libre_effects_editor_model::preview_scene::PreviewRequest {
            project: project.clone(),
            frame,
            dimension: options.dimension,
            revision: 0,
            document_revision: 0,
            core_generation: 0,
            transport: 0,
            gradient_gesture: None,
        };
        request.validate_evaluated_view(fresh.evaluated.as_deref())?;
        request.validate_evaluated_view(hit.evaluated.as_deref())?;
        let matching_geometry = fresh.evaluated == hit.evaluated;
        let shared_pixels = Arc::ptr_eq(&fresh.pixels, &hit.pixels);
        if !matching_geometry || !shared_pixels {
            return Err("Cached preview lost matching pixels or expression geometry".into());
        }
        measurements.push(Measurement {
            frame,
            width: hit.pixels.width(),
            height: hit.pixels.height(),
            render_milliseconds,
            cached_milliseconds,
            shared_pixels,
            matching_geometry,
            evaluated_geometry: hit.evaluated.is_some(),
            hardware: libre_effects_gpu_render::status(),
            video_decoder: renderer.decoder_status(),
            stages,
            operations,
            filter_cache_hits: renderer.filter_cache_hits(),
            path_cache_hits: renderer.path_cache_hits(),
        });
    }
    if before != libre_effects_core::project_file::encode(&project, None)? {
        return Err("Preview benchmark modified authored project bytes".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&measurements).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn benchmark_admits_only_bounded_explicit_frame_requests() {
        assert_eq!(
            parse(args(&["fixture.lep", "--frames", "0,300,4360"])).unwrap(),
            Options {
                project: "fixture.lep".into(),
                frames: vec![0, 300, 4360],
                dimension: 1280,
                composition: None,
            }
        );
        for values in [
            vec![],
            vec!["fixture.lep"],
            vec!["fixture.lep", "--frames", "-1"],
            vec!["fixture.lep", "--frames", "0,,1"],
            vec!["fixture.lep", "--frames", "0", "--frames", "1"],
            vec!["fixture.lep", "--frames", "0", "--dimension", "0"],
            vec!["fixture.lep", "--frames", "0", "--dimension", "8193"],
            vec![
                "fixture.lep",
                "--frames",
                "0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16",
            ],
        ] {
            assert!(parse(args(&values)).is_err(), "{values:?}");
        }
    }
}
