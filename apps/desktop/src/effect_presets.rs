use libre_effects_core::EffectPreset;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(crate) struct Entry {
    pub id: PathBuf,
    pub preset: EffectPreset,
}
#[derive(Default)]
pub(crate) struct Library {
    pub entries: Vec<Entry>,
    pub busy: bool,
    pub message: String,
}
pub(crate) fn root() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_STATE_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("LibreEffects/effect-presets"))
        .ok_or("User data directory unavailable".into())
}
pub(crate) fn read(path: &Path) -> Result<EffectPreset, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(EffectPreset::MAX_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    EffectPreset::from_json(&text)
}
pub(crate) fn load(root: &Path) -> Result<(Vec<Entry>, String), String> {
    if !root.exists() {
        return Ok((vec![], String::new()));
    }
    let mut paths = std::fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".lfe-preset.json"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut entries = vec![];
    let mut skipped = 0;
    let mut bytes = 0;
    for path in paths {
        let len = std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(u64::MAX);
        if entries.len() >= 200
            || len > EffectPreset::MAX_BYTES as u64
            || bytes + len > 32 * 1024 * 1024
        {
            skipped += 1;
            continue;
        }
        bytes += len;
        match read(&path) {
            Ok(preset) => entries.push(Entry { id: path, preset }),
            Err(_) => skipped += 1,
        }
    }
    entries.sort_by(|a, b| {
        a.preset
            .name()
            .to_lowercase()
            .cmp(&b.preset.name().to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok((
        entries,
        if skipped == 0 {
            String::new()
        } else {
            format!("Skipped {skipped} invalid or oversized presets (200 files / 32 MiB limit).")
        },
    ))
}
pub(crate) fn register(root: &Path, preset: &EffectPreset) -> Result<(), String> {
    let data = preset.to_json()?;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let (entries, _) = load(root)?;
    if entries.iter().any(|e| &e.preset == preset) {
        return Ok(());
    }
    if entries.len() >= 200
        || entries
            .iter()
            .filter_map(|e| std::fs::metadata(&e.id).ok())
            .map(|m| m.len())
            .sum::<u64>()
            + data.len() as u64
            > 32 * 1024 * 1024
    {
        return Err("Preset library is full (200 files / 32 MiB).".into());
    }
    let mut file = tempfile::Builder::new()
        .prefix("preset-")
        .suffix(".lfe-preset.json")
        .tempfile_in(root)
        .map_err(|e| e.to_string())?;
    file.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.keep().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor, EffectEdit, EffectKind};
    #[test]
    fn library_import_is_validated_deduplicated_and_keeps_named_variants() {
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::Fill),
        })
        .unwrap();
        let p = EffectPreset::capture(
            e.project().composition().layer(1).unwrap(),
            None,
            30.into(),
            "My Fill",
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        register(dir.path(), &p).unwrap();
        register(dir.path(), &p).unwrap();
        register(dir.path(), &p.clone().renamed("Another Fill").unwrap()).unwrap();
        std::fs::write(dir.path().join("bad.lfe-preset.json"), "{invalid}").unwrap();
        let (entries, warning) = load(dir.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].preset.name(), "Another Fill");
        assert!(warning.contains("Skipped 1"));
        assert_eq!(read(&entries[1].id).unwrap(), p);
    }
    #[test]
    fn applied_preset_reproduces_source_pixels_after_save_reopen_and_png() {
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        for kind in [EffectKind::Fill, EffectKind::Tint] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
        }
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect: 1,
                parameter: libre_effects_core::EffectParam::Red,
                frame: 0,
                value: 30.0,
            },
        })
        .unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::ToggleAnimation {
                effect: 1,
                parameter: libre_effects_core::EffectParam::Red,
                frame: 0,
            },
        })
        .unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect: 1,
                parameter: libre_effects_core::EffectParam::Red,
                frame: 30,
                value: 240.0,
            },
        })
        .unwrap();
        let p = EffectPreset::capture(
            e.project().composition().layer(1).unwrap(),
            None,
            30.into(),
            "Look",
        )
        .unwrap();
        let r = crate::rendering::Renderer::new();
        let before = r.render_preview(e.project(), 15, 128).unwrap();
        e.execute(Command::AddSolid).unwrap();
        let saved = EffectPreset::from_json(&p.to_json().unwrap()).unwrap();
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::ApplyPreset {
                preset: saved,
                frame: 20,
            },
        })
        .unwrap();
        let doc = libre_effects_core::Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let after = r.render_preview(&doc, 35, 128).unwrap();
        assert_eq!(before, after);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preset.png");
        after.save(&path).unwrap();
        assert_eq!(image::open(path).unwrap().to_rgba8(), after);
    }
}
