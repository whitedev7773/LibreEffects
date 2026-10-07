//! Numbered stills remain linked files; no intermediate video or embedded frames.
use libre_effects_core::{Command, Content, FootageInterpretation, FrameRate, MissingFramePolicy};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(test)]
#[path = "image_sequence_tests.rs"]
mod tests;

fn numbered(path: &Path) -> Result<(String, usize, u64, String), String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .ok_or("Choose a numbered PNG or JPEG")?;
    if !matches!(
        extension.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg"
    ) {
        return Err("Image sequences support PNG and JPEG".into());
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Sequence filenames must be Unicode")?;
    let digits = stem.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 12 {
        return Err("Choose a filename ending in 1–12 digits, such as shot_0001.png".into());
    }
    let split = stem.len() - digits;
    Ok((
        stem[..split].into(),
        digits,
        stem[split..].parse().map_err(|_| "Invalid frame number")?,
        extension.into(),
    ))
}

pub(crate) fn discover(
    first: &Path,
    fps: FrameRate,
    folder: Option<u64>,
) -> Result<Command, String> {
    let first = std::fs::canonicalize(first).map_err(|e| e.to_string())?;
    let first = PathBuf::from(crate::media_io::path_string(&first)?);
    let (prefix, digits, start, extension) = numbered(&first)?;
    let directory = first.parent().ok_or("Sequence needs a parent folder")?;
    let mut found = BTreeMap::new();
    for (count, entry) in std::fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .enumerate()
    {
        if count >= 100_000 {
            return Err("Sequence folder exceeds 100,000 entries".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let Ok((p, d, number, ext)) = numbered(&entry.path()) else {
            continue;
        };
        if p == prefix
            && d == digits.max(number.to_string().len())
            && ext.eq_ignore_ascii_case(&extension)
            && number >= start
        {
            if found.insert(number, entry.path()).is_some() {
                return Err("Ambiguous duplicate sequence frame number".into());
            }
        }
    }
    let last = *found.last_key_value().ok_or("No sequence frames found")?.0;
    if last - start >= 100_000 {
        return Err("Sequence span exceeds 100,000 frames".into());
    }
    let mut frames = Vec::new();
    let mut dimensions = None;
    for number in start..=last {
        let path = found
            .get(&number)
            .cloned()
            .unwrap_or_else(|| directory.join(format!("{prefix}{number:0digits$}.{extension}")));
        if found.contains_key(&number) {
            let (_, w, h) = crate::rendering::import_image(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if dimensions.is_some_and(|size| size != (w, h)) {
                return Err(format!("Sequence dimensions differ: {}", path.display()));
            }
            dimensions = Some((w, h));
        }
        frames.push(crate::media_io::path_string(&path)?);
    }
    let (w, h) = dimensions.ok_or("No readable sequence frames")?;
    Ok(Command::ImportAsset {
        content: Content::ImageSequence {
            frames: Arc::new(frames),
            fps,
            missing: MissingFramePolicy::Error,
            start_frame: 0,
            playback: Default::default(),
        },
        width: f64::from(w),
        height: f64::from(h),
        name: format!("{prefix}[{start:0digits$}–{last:0digits$}].{extension}"),
        folder,
        frame: None,
    })
}

fn present(path: &str) -> Result<bool, String> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Ok(true),
        Ok(_) => Err(format!("Sequence source is not a file: {path}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("Cannot read sequence source {path}: {e}")),
    }
}
pub(crate) fn resolve_frame(
    frames: &[String],
    index: usize,
    missing: MissingFramePolicy,
) -> Result<Option<&str>, String> {
    let path = frames
        .get(index)
        .ok_or("Sequence frame is outside the manifest")?;
    if present(path)? {
        return Ok(Some(path));
    }
    match missing {
        MissingFramePolicy::Error => Err(format!(
            "Sequence frame offline: {path}. Relink or change Missing frames in Interpret footage."
        )),
        MissingFramePolicy::Transparent => Ok(None),
        MissingFramePolicy::Hold => {
            for path in frames[..index].iter().rev() {
                if present(path)? {
                    return Ok(Some(path));
                }
            }
            Ok(None)
        }
    }
}
pub(crate) fn frame_png(
    content: &Content,
    index: usize,
    width: u32,
    height: u32,
    interpretation: FootageInterpretation,
) -> Result<Option<String>, String> {
    let Content::ImageSequence {
        frames, missing, ..
    } = content
    else {
        return Err("Expected an image sequence".into());
    };
    let Some(path) = resolve_frame(frames, index, *missing)? else {
        return Ok(None);
    };
    let (image, w, h) = crate::rendering::import_image(Path::new(path))?;
    if (w, h) != (width, height) {
        return Err(format!("Sequence frame dimensions changed: {path}"));
    }
    let Content::Image { png } = image else {
        unreachable!()
    };
    Ok(Some(
        crate::source_render::alpha_png(&png, interpretation)?.into_owned(),
    ))
}
pub(crate) fn relocate(
    asset: u64,
    content: &Content,
    width: u32,
    height: u32,
    folder: &Path,
) -> Result<Command, String> {
    let Content::ImageSequence {
        frames, missing, ..
    } = content
    else {
        return Err("Expected an image sequence".into());
    };
    let folder = std::fs::canonicalize(folder).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    let mut available = 0;
    for original in frames.iter() {
        let name = Path::new(original)
            .file_name()
            .ok_or("Invalid sequence filename")?;
        let path = crate::media_io::path_string(&folder.join(name))?;
        if present(&path)? {
            let (_, w, h) = crate::rendering::import_image(Path::new(&path))?;
            if (w, h) != (width, height) {
                return Err(format!(
                    "Relink requires matching sequence dimensions: {path}"
                ));
            }
            available += 1;
        } else if *missing == MissingFramePolicy::Error {
            return Err(format!("Sequence frame not found: {path}"));
        }
        paths.push(path);
    }
    if available == 0 {
        return Err("No matching sequence frames in this folder".into());
    }
    Ok(Command::RelinkSequence {
        asset,
        frames: Arc::new(paths),
    })
}
