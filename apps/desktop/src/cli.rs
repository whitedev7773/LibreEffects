//! Deterministic file rendering without opening a window.
use std::{ffi::OsString, path::PathBuf};

const HELP: &str = "Libre Effects file renderer\n\
    --render PROJECT.lfe.json --output FILE.mp4|mov|png [--start FRAME] [--end FRAME] [--png-background]\n\
    Frame range is [start, end). Videos default to the entire composition; PNG defaults to one frame.\n\
    MP4 uses the composition background. MOV and PNG preserve alpha; --png-background makes PNG opaque.";

#[derive(Debug, PartialEq)]
struct Options {
    project: PathBuf,
    output: PathBuf,
    start: u32,
    end: Option<u32>,
    png_background: bool,
}

fn parse(args: Vec<OsString>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let (mut project, mut output, mut start, mut end) = (None, None, None, None);
    let mut png_background = false;
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("Invalid command-line option")?;
        if flag == "--png-background" && !png_background {
            png_background = true;
            continue;
        }
        match flag {
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
        project: project.ok_or("Use --render PROJECT.lfe.json")?,
        output: output.ok_or("Use --output FILE.mp4, FILE.mov or FILE.png")?,
        start: start.unwrap_or(0),
        end,
        png_background,
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
    let project = crate::project_io::read_project(&options.project)?;
    crate::project_io::protect_source(&options.output, &options.project)?;
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
    if extension == "png" {
        if end - options.start != 1 {
            return Err("PNG output requires exactly one frame".into());
        }
        crate::project_io::validate_render(&project, &options.output, &(options.start..end))?;
        let mut pixels =
            crate::rendering::Renderer::new().render(&project, options.start, u32::MAX)?;
        if options.png_background {
            crate::rendering::composite_background(
                &mut pixels,
                project.composition().background_color(),
            );
        }
        let mut data = std::io::Cursor::new(Vec::new());
        pixels
            .write_to(&mut data, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        crate::project_io::write_bytes(&options.output, &data.into_inner())?;
    } else {
        let preset = if extension == "mp4" {
            crate::video_export::VideoPreset::H264
        } else {
            crate::video_export::VideoPreset::ProResAlpha
        };
        crate::video_export::export_video(
            &project,
            options.start..end,
            preset,
            &options.output,
            Default::default(),
            Default::default(),
        )?;
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
    fn render_arguments_keep_unicode_paths_and_reject_ambiguous_ranges() {
        let args = [
            "--render",
            "한글 project.lfe.json",
            "--output",
            "render.png",
            "--start",
            "30",
            "--png-background",
        ];
        let parsed = parse(args.into_iter().map(OsString::from).collect()).unwrap();
        assert_eq!(parsed.project, PathBuf::from("한글 project.lfe.json"));
        assert_eq!(parsed.start, 30);
        assert!(parsed.png_background);
        for args in [
            vec!["--render"],
            vec!["--render", "x", "--output", "y", "--start", "-1"],
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
            png_background,
        };
        render(options(1, None, false)).unwrap();
        let alpha = image::open(&output_path).unwrap().to_rgba8();
        assert_eq!(alpha.get_pixel(0, 0)[3], 0);
        render(options(1, None, true)).unwrap();
        let opaque = image::open(&output_path).unwrap().to_rgba8();
        assert_eq!(opaque.get_pixel(0, 0).0, [0x12, 0x34, 0x56, 255]);
        let previous = std::fs::read(&output_path).unwrap();
        for (start, end) in [(2, None), (0, Some(2)), (1, Some(1)), (u32::MAX, None)] {
            assert!(render(options(start, end, false)).is_err());
            assert_eq!(std::fs::read(&output_path).unwrap(), previous);
        }
    }
}
