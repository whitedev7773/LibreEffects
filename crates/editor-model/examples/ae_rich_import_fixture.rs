//! Emit only independently authored fixtures for bounded native import QA.
use std::{fs, path::PathBuf};
fn main() -> Result<(), String> {
    let out = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Supply an output directory")?;
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let source = include_bytes!("../../ae-project/tests/fixtures/synthetic-native-ready.ae.json");
    let document = libre_effects_editor_model::ae_import::read_document(source)?;
    let roots = document.roots();
    assert_eq!(roots.len(), 3);
    assert!(roots[0].blocker.is_none());
    assert!(roots[1].blocker.is_none());
    assert!(roots[2].blocker.is_some());
    fs::write(out.join("rich-import.ae.json"), source).map_err(|e| e.to_string())?;
    for id in [101, 202] {
        let project = document.convert(id)?;
        fs::write(
            out.join(format!("root-{id}.lep")),
            libre_effects_core::project_file::encode(&project, None)?,
        )
        .map_err(|e| e.to_string())?;
    }
    println!(
        "Synthetic root101/root202 converted; root303 explicitly blocked. Output is a fixture, not an independent raster oracle."
    );
    Ok(())
}
