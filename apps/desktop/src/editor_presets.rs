use super::*;
use libre_effects_core::{EffectId, EffectPreset};

#[derive(Clone)]
pub(crate) enum PresetAction {
    Save(Option<EffectId>),
    Import,
    Reload,
    Apply(PathBuf),
}
impl EditorState {
    pub(crate) fn load_presets(&mut self, cx: &mut Context<Self>) {
        if self.presets.busy {
            return;
        }
        self.presets.busy = true;
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_executor()
                .spawn(async {
                    crate::effect_presets::root().and_then(|p| crate::effect_presets::load(&p))
                })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.presets.busy = false;
                match result {
                    Ok((entries, message)) => {
                        s.presets.entries = entries;
                        s.presets.message = message;
                    }
                    Err(e) => s.presets.message = e,
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn preset_action(
        &mut self,
        action: &PresetAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.presets.busy {
            self.status = "Wait for the preset operation to finish".into();
            return;
        }
        match action {
            PresetAction::Reload => self.load_presets(cx),
            PresetAction::Apply(path) => {
                let Some(entry) = self.presets.entries.iter().find(|e| &e.id == path) else {
                    self.status = "Preset no longer exists; refresh the list".into();
                    return;
                };
                let name = entry.preset.name().to_string();
                let ids: Vec<_> = if self.selected_layers.is_empty() {
                    self.editor.selected().into_iter().collect()
                } else {
                    self.selected_layers.iter().copied().collect()
                };
                if ids.is_empty() {
                    self.status = "Select a layer to apply the preset".into();
                    return;
                }
                let commands = ids
                    .into_iter()
                    .map(|id| Command::Effect {
                        id,
                        edit: libre_effects_core::EffectEdit::ApplyPreset {
                            preset: entry.preset.clone(),
                            frame: self.frame,
                        },
                    })
                    .collect();
                self.dispatch(&Action::Edit(Command::Batch(commands)), window, cx);
                if self.status == "Edited" {
                    self.status = format!("Applied {name}");
                    self.effect_controls_open = true;
                }
            }
            PresetAction::Save(effect) => {
                let Some(layer) = self.editor.selected_layer() else {
                    self.status = "Select a layer with effects".into();
                    return;
                };
                let preset = match EffectPreset::capture(
                    layer,
                    *effect,
                    self.editor.project().composition().fps(),
                    "Effects",
                ) {
                    Ok(p) => p,
                    Err(e) => {
                        self.status = e;
                        return;
                    }
                };
                self.stop();
                window.blur();
                let prompt =
                    cx.prompt_for_new_path(Path::new("."), Some("Effects.lfe-preset.json"));
                self.presets.busy = true;
                cx.spawn(async move |entity, cx| {
                    let result = match prompt.await {
                        Ok(Ok(Some(path))) => {
                            cx.background_executor()
                                .spawn(async move {
                                    let file = path
                                        .file_name()
                                        .and_then(|p| p.to_str())
                                        .ok_or("Invalid preset file name")?;
                                    let name = file
                                        .strip_suffix(".lfe-preset.json")
                                        .or_else(|| file.strip_suffix(".json"))
                                        .unwrap_or(file);
                                    let preset = preset.renamed(name)?;
                                    crate::project_io::write_bytes(
                                        &path,
                                        preset.to_json()?.as_bytes(),
                                    )?;
                                    crate::effect_presets::root()
                                        .and_then(|root| {
                                            crate::effect_presets::register(&root, &preset)
                                        })
                                        .map_err(|e| {
                                            format!("File saved; library registration failed: {e}")
                                        })?;
                                    Ok::<_, String>("Effect preset saved".to_string())
                                })
                                .await
                        }
                        _ => Ok("Preset save canceled".into()),
                    };
                    let _ = entity.update(cx, |s, cx| {
                        s.presets.busy = false;
                        s.status = result.unwrap_or_else(|e| e);
                        s.load_presets(cx);
                        cx.notify();
                    });
                })
                .detach();
            }
            PresetAction::Import => {
                window.blur();
                let prompt = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Import Libre Effects preset".into()),
                });
                self.presets.busy = true;
                cx.spawn(async move |entity, cx| {
                    let result = match prompt.await {
                        Ok(Ok(Some(paths))) if paths.len() == 1 => {
                            let path = paths[0].clone();
                            cx.background_executor()
                                .spawn(async move {
                                    let preset = crate::effect_presets::read(&path)?;
                                    crate::effect_presets::register(
                                        &crate::effect_presets::root()?,
                                        &preset,
                                    )?;
                                    Ok::<_, String>(format!("Imported {}", preset.name()))
                                })
                                .await
                        }
                        _ => Ok("Preset import canceled".into()),
                    };
                    let _ = entity.update(cx, |s, cx| {
                        s.presets.busy = false;
                        s.status = result.unwrap_or_else(|e| e);
                        s.load_presets(cx);
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }
}
