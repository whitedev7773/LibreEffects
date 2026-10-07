//! Exact, bounded qualification of saved projects against captured RGBA PNGs.
//! A completed mismatch writes its receipt and returns failure; there is no
//! tolerance, resampling, background compositing or hidden-RGB suppression.
use image::{ColorType, ImageDecoder, RgbaImage, codecs::png::PngDecoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

const MAX_CASES_BYTES: u64 = 1024 * 1024;
const MAX_PNG_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PIXELS: u64 = 33_554_432;
const MAX_JOB_PIXELS: u64 = 134_217_728;

#[derive(Debug, PartialEq)]
struct Options {
    project: PathBuf,
    cases: PathBuf,
    output: PathBuf,
}

fn parse(args: Vec<OsString>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let project = PathBuf::from(args.next().ok_or("Comparison requires PROJECT.lep")?);
    let (mut cases, mut output) = (None, None);
    while let Some(flag) = args.next() {
        let target = match flag.to_str() {
            Some("--cases") if cases.is_none() => &mut cases,
            Some("--output") if output.is_none() => &mut output,
            _ => return Err("Unknown or repeated reference comparison option".into()),
        };
        *target = Some(PathBuf::from(
            args.next().ok_or("Comparison option requires a path")?,
        ));
    }
    Ok(Options {
        project,
        cases: cases.ok_or("Use --cases CASES.json")?,
        output: output.ok_or("Use --output NEW_REPORT.json")?,
    })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    composition: u64,
    frame: u32,
    reference: PathBuf,
}

fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("Input exceeds byte budget: {}", path.display()));
    }
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    let mut decoder = PngDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("Reference PNG exceeds 8192 pixels per axis or 32 megapixels".into());
    }
    if decoder.color_type() != ColorType::Rgba8 || decoder.is_apng().map_err(|e| e.to_string())? {
        return Err(
            "Reference must be a static 8-bit RGBA PNG; capture without implicit color conversion"
                .into(),
        );
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_PNG_BYTES);
    decoder.set_limits(limits).map_err(|e| e.to_string())?;
    let mut pixels = vec![0; decoder.total_bytes() as usize];
    decoder.read_image(&mut pixels).map_err(|e| e.to_string())?;
    RgbaImage::from_raw(width, height, pixels).ok_or("Invalid reference pixel buffer".into())
}

#[derive(Debug, PartialEq, Serialize)]
struct Difference {
    pixels: u64,
    differing_pixels: u64,
    mean_absolute_rgba: [f64; 4],
    root_mean_square_rgba: [f64; 4],
    maximum_rgba: [u8; 4],
    // Exclusive right/bottom bounds, absent only for complete equality.
    bounds: Option<[u32; 4]>,
    first: Option<PixelDifference>,
}
#[derive(Debug, PartialEq, Serialize)]
struct PixelDifference {
    x: u32,
    y: u32,
    reference: [u8; 4],
    native: [u8; 4],
}

fn compare(reference: &RgbaImage, native: &RgbaImage) -> Result<Difference, String> {
    if reference.dimensions() != native.dimensions() {
        return Err("Comparison dimensions differ; resampling is not permitted".into());
    }
    let pixels = u64::from(reference.width()) * u64::from(reference.height());
    if pixels == 0 {
        return Err("Comparison requires nonempty images".into());
    }
    let (mut sums, mut squares, mut maximum) = ([0u64; 4], [0u64; 4], [0u8; 4]);
    let mut different = 0;
    let mut bounds = [reference.width(), reference.height(), 0, 0];
    let mut first = None;
    for (index, (a, b)) in reference.pixels().zip(native.pixels()).enumerate() {
        for c in 0..4 {
            let error = a.0[c].abs_diff(b.0[c]);
            sums[c] += u64::from(error);
            squares[c] += u64::from(error).pow(2);
            maximum[c] = maximum[c].max(error);
        }
        if a != b {
            different += 1;
            let x = (index % reference.width() as usize) as u32;
            let y = (index / reference.width() as usize) as u32;
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x + 1);
            bounds[3] = bounds[3].max(y + 1);
            first.get_or_insert(PixelDifference {
                x,
                y,
                reference: a.0,
                native: b.0,
            });
        }
    }
    Ok(Difference {
        pixels,
        differing_pixels: different,
        mean_absolute_rgba: sums.map(|v| v as f64 / pixels as f64),
        root_mean_square_rgba: squares.map(|v| (v as f64 / pixels as f64).sqrt()),
        maximum_rgba: maximum,
        bounds: (different != 0).then_some(bounds),
        first,
    })
}

#[derive(Serialize)]
struct Measurement {
    case: Case,
    width: u32,
    height: u32,
    reference_file_sha256: String,
    reference_rgba_sha256: String,
    native_rgba_sha256: String,
    difference: Difference,
}
#[derive(Serialize)]
struct Report {
    version: u32,
    comparison: &'static str,
    project_sha256: String,
    cases_sha256: String,
    authored_project_unchanged: bool,
    all_pixels_equal: bool,
    measurements: Vec<Measurement>,
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temp = tempfile::NamedTempFile::new_in(crate::output_preflight::parent(path))
        .map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn run(args: Vec<OsString>) -> Result<(), String> {
    execute(parse(args)?)
}

fn execute(options: Options) -> Result<(), String> {
    if options.output.exists() {
        return Err("Comparison report must be a new file".into());
    }
    crate::project_io::protect_source(&options.output, &options.project)?;
    crate::project_io::protect_source(&options.output, &options.cases)?;
    let cases_bytes = read(&options.cases, MAX_CASES_BYTES)?;
    let mut cases: Vec<Case> = serde_json::from_slice(
        cases_bytes
            .strip_prefix(b"\xef\xbb\xbf")
            .unwrap_or(&cases_bytes),
    )
    .map_err(|e| e.to_string())?;
    if cases.is_empty() || cases.len() > 64 {
        return Err("Comparison requires 1..64 frame cases".into());
    }
    let project_bytes = crate::project_io::read_bytes(&options.project)?;
    let opened = crate::project_io::decode_project(&project_bytes)?;
    let mut project = crate::media_io::resolve(&opened.project, &options.project)?;
    let original_composition = project.active_composition_id();
    let before = libre_effects_core::project_file::encode(&project, None)?;
    let mut total_pixels = 0u64;
    // Validate the complete job before rendering; decode only one PNG at a time.
    let mut reference_hashes = Vec::new();
    for case in &mut cases {
        if case.reference.is_relative() {
            case.reference = crate::output_preflight::parent(&options.cases).join(&case.reference);
        }
        crate::project_io::protect_source(&options.output, &case.reference)?;
        project.activate_composition(case.composition)?;
        if case.frame >= project.composition().duration() {
            return Err("Reference frame is outside its composition".into());
        }
        let bytes = read(&case.reference, MAX_PNG_BYTES)?;
        let image = decode(&bytes)?;
        if image.dimensions()
            != (
                project.composition().width(),
                project.composition().height(),
            )
        {
            return Err(
                "Reference dimensions must equal the composition's full output size".into(),
            );
        }
        total_pixels += u64::from(image.width()) * u64::from(image.height());
        if total_pixels > MAX_JOB_PIXELS {
            return Err("Comparison job exceeds 128 megapixels".into());
        }
        crate::output_preflight::check(
            &project,
            Some(&options.project),
            case.frame..case.frame + 1,
            crate::output_settings::Format::PngAlpha,
            &crate::output_settings::Settings {
                fonts: crate::output_settings::FontPolicy::Strict,
                ..Default::default()
            },
            &options.output,
            crate::output_preflight::Destination::File,
            &crate::video_export::ffmpeg_path(),
            &Default::default(),
        )
        .map_err(|e| e.to_string())?;
        reference_hashes.push(hash(&bytes));
    }
    let renderer = crate::rendering::Renderer::new();
    let mut measurements = Vec::new();
    for (case, expected_hash) in cases.into_iter().zip(reference_hashes) {
        project.activate_composition(case.composition)?;
        let bytes = read(&case.reference, MAX_PNG_BYTES)?;
        if hash(&bytes) != expected_hash {
            return Err("Reference file changed during comparison".into());
        }
        let reference = decode(&bytes)?;
        let native =
            renderer.render_output(&project, case.frame, reference.width(), reference.height())?;
        let difference = compare(&reference, &native)?;
        eprintln!(
            "Compared composition {} frame {}: {} / {} pixels differ",
            case.composition, case.frame, difference.differing_pixels, difference.pixels
        );
        measurements.push(Measurement {
            width: native.width(),
            height: native.height(),
            reference_file_sha256: expected_hash,
            reference_rgba_sha256: hash(reference.as_raw()),
            native_rgba_sha256: hash(native.as_raw()),
            case,
            difference,
        });
    }
    project.activate_composition(original_composition)?;
    if before != libre_effects_core::project_file::encode(&project, None)?
        || project_bytes != crate::project_io::read_bytes(&options.project)?
    {
        return Err("Authored project changed during reference comparison".into());
    }
    let all_pixels_equal = measurements
        .iter()
        .all(|m| m.difference.differing_pixels == 0);
    let report = Report {
        version: 1,
        comparison: "exact decoded 8-bit RGBA; full composition; strict fonts",
        project_sha256: hash(&project_bytes),
        cases_sha256: hash(&cases_bytes),
        authored_project_unchanged: true,
        all_pixels_equal,
        measurements,
    };
    write_new(
        &options.output,
        &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )?;
    if !all_pixels_equal {
        return Err(format!(
            "Reference pixels differ; complete report: {}",
            options.output.display()
        ));
    }
    println!(
        "All reference pixels match; report: {}",
        options.output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;
    #[test]
    fn exact_rgba_counts_hidden_color_alpha_and_spatial_bounds() {
        let reference = RgbaImage::from_raw(
            2,
            2,
            vec![
                10, 20, 30, 0, 10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255,
            ],
        )
        .unwrap();
        let native = RgbaImage::from_raw(
            2,
            2,
            vec![
                11, 20, 30, 0, 10, 20, 30, 255, 10, 20, 30, 255, 10, 22, 30, 253,
            ],
        )
        .unwrap();
        let result = compare(&reference, &native).unwrap();
        assert_eq!(result.differing_pixels, 2);
        assert_eq!(result.mean_absolute_rgba, [0.25, 0.5, 0.0, 0.5]);
        assert_eq!(result.root_mean_square_rgba, [0.5, 1.0, 0.0, 1.0]);
        assert_eq!(result.maximum_rgba, [1, 2, 0, 2]);
        assert_eq!(result.bounds, Some([0, 0, 2, 2]));
        assert_eq!(result.first.unwrap().reference, [10, 20, 30, 0]);
        let equal = compare(&reference, &reference).unwrap();
        assert_eq!(equal.differing_pixels, 0);
        assert_eq!(equal.bounds, None);
        assert_eq!(equal.first, None);
        assert!(compare(&reference, &RgbaImage::new(1, 4)).is_err());
    }
    #[test]
    fn comparison_requires_explicit_cases_and_new_output_and_rejects_implicit_conversion() {
        let args = |v: &[&str]| v.iter().map(OsString::from).collect();
        assert!(parse(args(&["a.lep", "--cases", "b.json", "--output", "c.json"])).is_ok());
        for v in [
            vec![],
            vec!["a.lep"],
            vec!["a.lep", "--cases", "b.json", "--cases", "d.json"],
            vec!["a.lep", "--tolerance", "1"],
        ] {
            assert!(parse(args(&v)).is_err());
        }
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&[1, 2, 3], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert!(decode(&bytes).unwrap_err().contains("RGBA"));
        let mut sixteen_bit = Vec::new();
        image::codecs::png::PngEncoder::new(&mut sixteen_bit)
            .write_image(&[0; 8], 1, 1, image::ExtendedColorType::Rgba16)
            .unwrap();
        assert!(decode(&sixteen_bit).unwrap_err().contains("8-bit"));
        assert!(
            serde_json::from_str::<Case>(
                r#"{"composition":1,"frame":0,"reference":"a.png","tolerance":1}"#
            )
            .is_err()
        );
    }
    #[test]
    fn complete_native_job_reports_mismatch_preserves_inputs_and_never_replaces_receipts() {
        use libre_effects_core::{Command, Editor};
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("test.lep");
        let cases_path = dir.path().join("cases.json");
        let reference_path = dir.path().join("reference.png");
        let output = dir.path().join("receipt.json");
        let mut editor = Editor::default();
        editor
            .execute(Command::ConfigureComposition {
                name: "Reference".into(),
                width: 2,
                height: 2,
                fps: 30,
                duration: 2,
            })
            .unwrap();
        editor.execute(Command::AddSolid).unwrap();
        editor
            .execute(Command::SetColor {
                id: 1,
                color: 0x123456,
            })
            .unwrap();
        crate::project_io::write_native_project(&project_path, editor.project(), None).unwrap();
        let original = std::fs::read(&project_path).unwrap();
        let write_reference = |pixels: &[u8]| {
            let file = std::fs::File::create(&reference_path).unwrap();
            image::codecs::png::PngEncoder::new(file)
                .write_image(pixels, 2, 2, image::ExtendedColorType::Rgba8)
                .unwrap();
        };
        write_reference(&[0x12, 0x34, 0x56, 255].repeat(4));
        let mut cases_bytes = b"\xef\xbb\xbf".to_vec();
        cases_bytes.extend(
            serde_json::to_vec(&vec![Case {
                composition: editor.project().active_composition_id(),
                frame: 0,
                reference: "reference.png".into(),
            }])
            .unwrap(),
        );
        std::fs::write(&cases_path, &cases_bytes).unwrap();
        let options = || Options {
            project: project_path.clone(),
            cases: cases_path.clone(),
            output: output.clone(),
        };
        execute(options()).unwrap();
        let receipt = std::fs::read(&output).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&receipt).unwrap()["all_pixels_equal"],
            true
        );
        assert!(execute(options()).unwrap_err().contains("new file"));
        assert_eq!(std::fs::read(&output).unwrap(), receipt);
        let different_output = dir.path().join("different.json");
        let mut pixels = [0x12, 0x34, 0x56, 255].repeat(4);
        pixels[15] = 254;
        write_reference(&pixels);
        assert!(
            execute(Options {
                output: different_output.clone(),
                ..options()
            })
            .unwrap_err()
            .contains("pixels differ")
        );
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(different_output).unwrap()).unwrap();
        assert_eq!(
            report["measurements"][0]["difference"]["differing_pixels"],
            1
        );
        assert_eq!(
            report["measurements"][0]["difference"]["maximum_rgba"],
            serde_json::json!([0, 0, 0, 1])
        );
        assert_eq!(std::fs::read(&project_path).unwrap(), original);
        assert_eq!(
            decode(&std::fs::read(reference_path).unwrap())
                .unwrap()
                .into_raw(),
            pixels
        );
        let invalid_output = dir.path().join("invalid.json");
        assert_eq!(std::fs::read(&cases_path).unwrap(), cases_bytes);
        let mut invalid_case: serde_json::Value =
            serde_json::from_slice(&cases_bytes[3..]).unwrap();
        let mut invalid_second = invalid_case[0].clone();
        invalid_second["frame"] = serde_json::json!(2);
        invalid_case.as_array_mut().unwrap().push(invalid_second);
        std::fs::write(&cases_path, serde_json::to_vec(&invalid_case).unwrap()).unwrap();
        assert!(
            execute(Options {
                output: invalid_output.clone(),
                ..options()
            })
            .unwrap_err()
            .contains("outside")
        );
        assert!(!invalid_output.exists());
    }
}
