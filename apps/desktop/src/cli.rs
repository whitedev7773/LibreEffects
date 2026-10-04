//! Deterministic file rendering without opening a window.
use crate::output_settings::{Field, Format, Settings};
use std::{ffi::OsString, path::PathBuf};

const HELP: &str = "Libre Effects file renderer\n\
    --render PROJECT.lep --output FILE.mp4|mov|png [--composition ID] [--start FRAME] [--end FRAME] [--png-background] [--size WIDTHxHEIGHT] [--fps RATE] [--channels auto|rgb|rgba|alpha] [--crf 0..51 | --bitrate KBPS] [--encoder SPEED] [--audio auto|off] [--fonts fallback|strict]\n\
    Native .lep and legacy JSON projects are detected by file contents.\n\
    Frame range is [start, end). Videos default to the entire composition; PNG defaults to one frame.\n\
    Fonts default to fallback with warnings; strict requires the requested family/style.\n\
    MP4 uses the composition background. MOV and PNG preserve alpha; --png-background makes PNG opaque.";

#[derive(Debug, PartialEq)]
struct Options {
    project: PathBuf,
    output: PathBuf,
    start: u32,
    end: Option<u32>,
    composition: Option<u64>,
    png_background: bool,
    settings: Settings,
}

fn parse(args: Vec<OsString>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let (mut project, mut output, mut start, mut end) = (None, None, None, None);
    let mut png_background = false;
    let mut composition = None;
    let mut settings = Settings::default();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("Invalid command-line option")?;
        if flag == "--png-background" && !png_background {
            png_background = true;
            continue;
        }
        match flag {
            "--size" | "--fps" | "--channels" | "--crf" | "--bitrate" | "--encoder" | "--audio"
            | "--fonts" => {
                let field = match flag {
                    "--size" => Field::Size,
                    "--fps" => Field::Fps,
                    "--channels" => Field::Channels,
                    "--encoder" => Field::Speed,
                    "--audio" => Field::Audio,
                    "--fonts" => Field::Fonts,
                    _ => Field::Quality,
                };
                if !seen.insert(field) {
                    return Err(format!("Repeated or conflicting option: {flag}"));
                }
                let value = args
                    .next()
                    .and_then(|v| v.into_string().ok())
                    .ok_or_else(|| format!("{flag} requires a value"))?;
                let value = match flag {
                    "--crf" => format!("crf:{value}"),
                    "--bitrate" => format!("kbps:{value}"),
                    _ => value,
                };
                settings.change(field, &value)?;
            }
            "--composition" if composition.is_none() => {
                composition = Some(
                    args.next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse::<u64>().ok()))
                        .filter(|id| *id > 0)
                        .ok_or("--composition requires a positive composition ID")?,
                );
            }
            "--render" if project.is_none() => {
                project = Some(PathBuf::from(
                    args.next().ok_or("--render requires a project path")?,
                ));
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--output requires a destination")?,
                ));
            }
            "--start" | "--end" => {
                let target = if flag == "--start" {
                    &mut start
                } else {
                    &mut end
                };
                if target.is_some() {
                    return Err(format!("Repeated option: {flag}"));
                }
                *target = Some(
                    args.next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse::<u32>().ok()))
                        .ok_or_else(|| {
                            format!("{flag} requires a whole nonnegative frame number")
                        })?,
                );
            }
            _ => return Err(format!("Unknown or repeated option: {flag}")),
        }
    }
    Ok(Options {
        project: project.ok_or("Use --render PROJECT.lep")?,
        output: output.ok_or("Use --output FILE.mp4, FILE.mov or FILE.png")?,
        start: start.unwrap_or(0),
        end,
        composition,
        png_background,
        settings,
    })
}

fn render(options: Options) -> Result<(), String> {
    let extension = options
        .output
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["png", "mp4", "mov"].contains(&extension.as_str()) {
        return Err("Output must end in .png, .mp4 or .mov".into());
    }
    if options.png_background && extension != "png" {
        return Err("--png-background applies only to PNG output".into());
    }
    let format = match extension.as_str() {
        "mp4" => Format::Mp4,
        "mov" => Format::MovAlpha,
        _ => {
            if options.png_background {
                Format::PngBackground
            } else {
                Format::PngAlpha
            }
        }
    };
    if options.png_background && options.settings.channels != crate::output_settings::Channels::Auto
    {
        return Err("Use either --png-background or --channels".into());
    }
    if extension == "png" && options.settings.fps.is_some() {
        return Err("--fps requires video or a queue PNG sequence".into());
    }
    let mut project = crate::project_io::read_project(&options.project)?;
    if let Some(id) = options.composition {
        project.activate_composition(id)?;
    }
    let end = options.end.unwrap_or_else(|| {
        if extension == "png" {
            options.start.saturating_add(1)
        } else {
            project.composition().duration()
        }
    });
    if options.start >= end || end > project.composition().duration() {
        return Err("Frame range must be nonempty and inside the composition".into());
    }
    let report = if extension == "png" {
        if end - options.start != 1 {
            return Err("PNG output requires exactly one frame".into());
        }
        let prepared = crate::output_preflight::check(
            &project,
            Some(&options.project),
            options.start..end,
            format,
            &options.settings,
            &options.output,
            crate::output_preflight::Destination::File,
            &crate::video_export::ffmpeg_path(),
            &Default::default(),
        )
        .map_err(|report| report.to_string())?;
        let plan = prepared.plan;
        let mut pixels = crate::rendering::Renderer::new().render_output(
            &project,
            options.start,
            plan.width,
            plan.height,
        )?;
        options.settings.apply_channels(
            &mut pixels,
            format,
            project.composition().background_color(),
        );
        crate::project_io::write_bytes(
            &options.output,
            &options.settings.png_bytes(pixels, format)?,
        )?;
        prepared.report
    } else {
        let preset = if extension == "mp4" {
            crate::video_export::VideoPreset::H264
        } else {
            crate::video_export::VideoPreset::ProResAlpha
        };
        crate::video_export::export_video_to(
            &project,
            Some(&options.project),
            options.start..end,
            preset,
            &options.settings,
            &options.output,
            Default::default(),
            Default::default(),
        )?
    };
    if !report.diagnostics.is_empty() {
        eprintln!("{report}");
    }
    println!(
        "Rendered frames {}..{} to {}",
        options.start,
        end,
        options.output.display()
    );
    Ok(())
}

pub(crate) fn run() -> Result<bool, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        return Ok(false);
    }
    if args.len() == 1 && args[0] == "--help" {
        println!("{HELP}");
        return Ok(true);
    }
    render(parse(args)?)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trim_failure_keeps_existing_still_and_leaves_no_temporary_output() {
        for (e, budget, kind) in crate::rendering::trim_tests::failure_cases() {
            let dir = tempfile::tempdir().unwrap();
            let project = dir.path().join("trim.lep");
            let output = dir.path().join("keep.png");
            crate::project_io::write_native_project(&project, e.project(), None).unwrap();
            std::fs::write(&output, b"existing still").unwrap();
            let error = crate::rendering::with_test_contents_budget(budget, || {
                render(Options {
                    project: project.clone(),
                    output: output.clone(),
                    start: 0,
                    end: Some(1),
                    composition: None,
                    png_background: false,
                    settings: Default::default(),
                })
            })
            .unwrap_err();
            assert!(
                error.contains(kind) && error.contains("Trim line") && error.contains("frame 0"),
                "{error}"
            );
            assert_eq!(std::fs::read(&output).unwrap(), b"existing still");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        }
    }
    #[test]
    fn output_options_parse_exact_rates_and_reject_conflicting_rate_control() {
        let args = [
            "--render",
            "x.lfe.json",
            "--output",
            "out.mp4",
            "--size",
            "1280x720",
            "--fps",
            "29.97",
            "--crf",
            "22",
            "--encoder",
            "slow",
            "--channels",
            "rgb",
        ];
        let parsed = parse(args.into_iter().map(OsString::from).collect()).unwrap();
        assert_eq!(parsed.settings.size, Some([1280, 720]));
        assert_eq!(parsed.settings.fps.unwrap().to_string(), "30000/1001");
        assert_eq!(
            parsed.settings.rate_control,
            Some(crate::output_settings::RateControl::Crf(22))
        );
        for extra in [
            vec!["--crf", "20", "--bitrate", "8000"],
            vec!["--fps", "24", "--fps", "30"],
            vec!["--size", "1280"],
            vec!["--encoder"],
        ] {
            let mut args = vec!["--render", "x", "--output", "y"];
            args.extend(extra);
            assert!(parse(args.into_iter().map(OsString::from).collect()).is_err());
        }
    }
    #[test]
    fn render_arguments_keep_unicode_paths_and_reject_ambiguous_ranges() {
        let args = [
            "--render",
            "한글 project.lfe.json",
            "--output",
            "render.png",
            "--start",
            "30",
            "--png-background",
            "--composition",
            "2",
        ];
        let parsed = parse(args.into_iter().map(OsString::from).collect()).unwrap();
        assert_eq!(parsed.project, PathBuf::from("한글 project.lfe.json"));
        assert_eq!(parsed.start, 30);
        assert!(parsed.png_background);
        assert_eq!(parsed.composition, Some(2));
        for args in [
            vec!["--render"],
            vec!["--render", "x", "--output", "y", "--start", "-1"],
            vec!["--render", "x", "--output", "y", "--composition", "0"],
            vec![
                "--render",
                "x",
                "--output",
                "y",
                "--composition",
                "1",
                "--composition",
                "2",
            ],
            vec![
                "--render", "x", "--output", "y", "--start", "0", "--start", "1",
            ],
        ] {
            assert!(parse(args.into_iter().map(OsString::from).collect()).is_err());
        }
    }
    #[test]
    fn headless_png_output_preserves_alpha_or_composites_and_failures_keep_destination() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("project.lfe.json");
        let output_path = dir.path().join("frame.png");
        let mut editor = libre_effects_core::Editor::default();
        editor
            .execute(libre_effects_core::Command::ConfigureComposition {
                name: "CLI".into(),
                width: 16,
                height: 16,
                fps: 30,
                duration: 2,
            })
            .unwrap();
        editor
            .execute(libre_effects_core::Command::SetCompositionBackground(
                0x123456,
            ))
            .unwrap();
        crate::project_io::write_project(&project_path, &editor.project().to_json().unwrap())
            .unwrap();
        let options = |start, end, png_background| Options {
            project: project_path.clone(),
            output: output_path.clone(),
            start,
            end,
            composition: None,
            png_background,
            settings: Settings::default(),
        };
        render(options(1, None, false)).unwrap();
        let alpha = image::open(&output_path).unwrap().to_rgba8();
        assert_eq!(alpha.get_pixel(0, 0)[3], 0);
        render(options(1, None, true)).unwrap();
        let opaque = image::open(&output_path).unwrap().to_rgba8();
        assert_eq!(opaque.get_pixel(0, 0).0, [0x12, 0x34, 0x56, 255]);
        let previous = std::fs::read(&output_path).unwrap();
        let mut scaled = options(1, None, false);
        scaled.settings.size = Some([8, 4]);
        scaled.settings.channels = crate::output_settings::Channels::Alpha;
        render(scaled).unwrap();
        let scaled = image::open(&output_path).unwrap().to_rgba8();
        assert_eq!(scaled.dimensions(), (8, 4));
        assert_eq!(scaled.get_pixel(0, 0).0, [0, 0, 0, 255]);
        std::fs::write(&output_path, &previous).unwrap();
        for field in [Field::Quality, Field::Fps] {
            let mut invalid = options(1, None, false);
            invalid
                .settings
                .change(
                    field,
                    if field == Field::Quality {
                        "crf:20"
                    } else {
                        "60"
                    },
                )
                .unwrap();
            assert!(render(invalid).is_err());
            assert_eq!(std::fs::read(&output_path).unwrap(), previous);
        }
        for (start, end) in [(2, None), (0, Some(2)), (1, Some(1)), (u32::MAX, None)] {
            assert!(render(options(start, end, false)).is_err());
            assert_eq!(std::fs::read(&output_path).unwrap(), previous);
        }
        editor
            .execute(libre_effects_core::Command::NewComposition)
            .unwrap();
        editor
            .execute(libre_effects_core::Command::SetCompositionBackground(
                0xff0000,
            ))
            .unwrap();
        crate::project_io::write_project(&project_path, &editor.project().to_json().unwrap())
            .unwrap();
        let mut first = options(0, None, true);
        first.composition = Some(1);
        render(first).unwrap();
        assert_eq!(
            image::open(&output_path)
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)
                .0,
            [0x12, 0x34, 0x56, 255]
        );
        render(options(0, None, true)).unwrap();
        assert_eq!(
            image::open(&output_path)
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)
                .0,
            [255, 0, 0, 255]
        );
        let previous = std::fs::read(&output_path).unwrap();
        let mut invalid = options(0, None, true);
        invalid.composition = Some(99);
        assert!(render(invalid).is_err());
        assert_eq!(std::fs::read(&output_path).unwrap(), previous);
    }
    #[test]
    fn cli_font_policy_strict_preserves_destination_and_fallback_renders() {
        use libre_effects_core::{Command, Content, Editor, TextStyle};
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("project.lfe.json");
        let output_path = directory.path().join("output.png");
        let mut editor = Editor::default();
        editor
            .execute(Command::ConfigureComposition {
                name: "CLI fonts".into(),
                width: 64,
                height: 48,
                fps: 24,
                duration: 2,
            })
            .unwrap();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Text".into(),
                    font_size: 16.0,
                },
                width: 60.0,
                height: 40.0,
                name: "Text".into(),
            })
            .unwrap();
        editor
            .execute(Command::SetTextStyle {
                id: 1,
                style: TextStyle {
                    font_family: "Missing CLI font QA".into(),
                    ..Default::default()
                },
            })
            .unwrap();
        crate::project_io::write_project(&project_path, &editor.project().to_json().unwrap())
            .unwrap();
        std::fs::write(&output_path, b"existing output").unwrap();
        let options = |policy: &str| {
            parse(vec![
                "--render".into(),
                project_path.clone().into_os_string(),
                "--output".into(),
                output_path.clone().into_os_string(),
                "--fonts".into(),
                policy.into(),
            ])
            .unwrap()
        };
        assert!(
            render(options("strict"))
                .unwrap_err()
                .contains("Fonts policy: strict")
        );
        assert_eq!(std::fs::read(&output_path).unwrap(), b"existing output");
        render(options("fallback")).unwrap();
        assert_eq!(image::open(&output_path).unwrap().width(), 64);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
        assert!(
            parse(
                ["--render", "x", "--output", "y", "--fonts", "silent"]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
            )
            .is_err()
        );
        assert!(
            parse(
                [
                    "--render", "x", "--output", "y", "--fonts", "strict", "--fonts", "fallback"
                ]
                .into_iter()
                .map(OsString::from)
                .collect()
            )
            .is_err()
        );
    }
    #[test]
    fn native_and_legacy_cli_reads_and_png_pixels_are_identical() {
        use libre_effects_core::{Command, Editor, Property};
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("legacy.json");
        let native = directory.path().join("native.lep");
        let mut editor = Editor::default();
        editor
            .execute(Command::ConfigureComposition {
                name: "Native CLI".into(),
                width: 32,
                height: 24,
                fps: 24,
                duration: 3,
            })
            .unwrap();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetColor {
                id: 1,
                color: 0x3388bb,
            })
            .unwrap();
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 40.0,
            })
            .unwrap();
        editor
            .execute(Command::SetCompositionBackground(0x123456))
            .unwrap();
        crate::project_io::write_project(&legacy, &editor.project().to_json().unwrap()).unwrap();
        crate::project_io::write_native_project(&native, editor.project(), None).unwrap();
        assert_eq!(
            crate::project_io::read_project(&legacy).unwrap(),
            crate::project_io::read_project(&native).unwrap()
        );
        for background in [false, true] {
            let mut pixels = Vec::new();
            for (index, source) in [&legacy, &native].into_iter().enumerate() {
                let output = directory.path().join(format!("render-{index}.png"));
                render(Options {
                    project: source.clone(),
                    output: output.clone(),
                    start: 1,
                    end: None,
                    composition: None,
                    png_background: background,
                    settings: Settings::default(),
                })
                .unwrap();
                pixels.push(image::open(output).unwrap().to_rgba8());
            }
            assert_eq!(pixels[0], pixels[1]);
        }
    }

    #[test]
    fn cli_detects_contents_and_preserves_sources_even_when_they_are_named_png() {
        let directory = tempfile::tempdir().unwrap();
        for native in [false, true] {
            let source = directory
                .path()
                .join(if native { "native.png" } else { "legacy.png" });
            let bytes = if native {
                crate::project_io::encode_native_project(
                    &libre_effects_core::Project::default(),
                    None,
                )
                .unwrap()
            } else {
                libre_effects_core::Project::default()
                    .to_json()
                    .unwrap()
                    .into_bytes()
            };
            std::fs::write(&source, &bytes).unwrap();
            let alias = directory.path().join(if native {
                "native-alias.png"
            } else {
                "legacy-alias.png"
            });
            std::fs::hard_link(&source, &alias).unwrap();
            for destination in [&source, &alias] {
                let error = render(Options {
                    project: source.clone(),
                    output: destination.clone(),
                    start: 0,
                    end: None,
                    composition: None,
                    png_background: false,
                    settings: Settings::default(),
                })
                .unwrap_err();
                assert!(error.contains("source"), "{error}");
                assert_eq!(std::fs::read(&source).unwrap(), bytes);
                assert_eq!(std::fs::read(&alias).unwrap(), bytes);
            }
        }
    }
}
