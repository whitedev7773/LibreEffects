//! Generate the independently authored, native lyric/part template.
#[path = "support/native_lyric_fixture.rs"]
mod native_lyric_fixture;

use libre_effects_core::project_file;
use native_lyric_fixture::{TemplateOptions, build};
use std::{io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let destination = PathBuf::from(args.next().ok_or(
        "Usage: native_lyric_template NEW.lep [--duration-seconds N] [--name-artist-fps 30|60] [--locked-song] [--without-lyric-expressions]",
    )?);
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lep"))
    {
        return Err("Native project examples use the .lep extension".into());
    }
    let mut options = TemplateOptions::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--duration-seconds" => {
                options.duration_seconds =
                    args.next().ok_or("Missing duration seconds")?.parse()?;
            }
            "--name-artist-fps" => {
                options.name_artist_fps =
                    args.next().ok_or("Missing Name & Artist FPS")?.parse()?;
            }
            "--locked-song" => options.lock_song = true,
            "--without-lyric-expressions" => options.lyric_expressions = false,
            _ => return Err(format!("Unknown template option: {arg}").into()),
        }
    }
    let editor = build(options)?;
    let bytes = project_file::encode(editor.project(), None)?;
    let decoded = project_file::decode(&bytes)?;
    if decoded.project != *editor.project()
        || project_file::encode(&decoded.project, None)? != bytes
    {
        return Err("Native template source roundtrip did not preserve exact bytes".into());
    }
    // Never replace a user's existing project or a previously generated variant.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    println!(
        "Wrote {} bytes to {} ({} seconds; Lyric 60fps; Name & Artist {}fps; Song locked: {})",
        bytes.len(),
        destination.display(),
        options.duration_seconds,
        options.name_artist_fps,
        options.lock_song,
    );
    Ok(())
}
