use super::*;

impl EditorState {
    pub(super) fn import_video(&mut self, relink: bool, cx: &mut Context<Self>) {
        if self.importing_video {
            return;
        }
        let original = if relink {
            let layer = self
                .editor
                .selected()
                .and_then(|id| self.editor.project().composition().layer(id));
            match layer {
                Some(l) if matches!(l.content(), Content::Video { .. }) && !l.locked() => {
                    Some(l.clone())
                }
                _ => {
                    self.status = "Select an unlocked video layer to relink".into();
                    return;
                }
            }
        } else {
            None
        };
        self.stop();
        let revision = self.document_revision;
        let frame = self.frame;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if relink {
                    "Relink video source"
                } else {
                    "Import local video"
                }
                .into(),
            ),
        });
        self.importing_video = true;
        self.status = "Choose video footage".into();
        cx.spawn(async move |entity, cx| {
            let selected = prompt.await.ok().and_then(Result::ok).flatten().and_then(|p| p.into_iter().next());
            let Some(path) = selected else {
                let _ = entity.update(cx, |s, cx| { s.importing_video = false; s.status = "Video import canceled".into(); cx.notify(); });
                return;
            };
            let _ = entity.update(cx, |s, cx| { s.status = "Reading video metadata…".into(); cx.notify(); });
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let result = cx.background_executor().spawn(async move {
                let info = crate::footage::probe(&path)?;
                // Verify the codec and first frame before adding a layer.
                crate::footage::frame_png(&info.path, 0.0, info.width, info.height, 320)?;
                Ok::<_, String>(info)
            }).await;
            let _ = entity.update(cx, |s, cx| {
                s.importing_video = false;
                let result = result.and_then(|info| {
                    if s.document_revision != revision { return Err("Document changed during import; import the video again".into()); }
                    let command = if let Some(old) = &original {
                        let current = s.editor.project().composition().layer(old.id()).ok_or("Layer was deleted during relink")?;
                        if current.content() != old.content() { return Err("Video source changed during relink".into()); }
                        if info.width as f64 != old.width() || info.height as f64 != old.height() { return Err("Relink requires the same source dimensions; import this file as a new layer".into()); }
                        let Content::Video { path: original, .. } = old.content() else { unreachable!() };
                        Command::RelinkMedia(vec![libre_effects_core::MediaReplacement { audio: info.audio, original: original.clone(), path: info.path, duration: info.duration, fps: info.source_fps, width: info.width, height: info.height }])
                    } else {
                        Command::AddContent { content: Content::Video { audio: info.audio, path: info.path, duration: info.duration, source_fps: info.source_fps, start_frame: frame as i64, playback: Default::default() }, width: info.width as f64, height: info.height as f64, name }
                    };
                    s.editor.execute(command)
                });
                s.status = match result {
                    Ok(()) => {
                        crate::footage::clear_cache();
                        s.preview_revision = s.preview_revision.wrapping_add(1);
                        s.selected_layers.clear();
                        "Video linked · audio metadata imported · playback and export remain silent".into()
                    },
                    Err(e) => format!("Video import failed: {e}"),
                };
                s.normalize();
                cx.notify();
            });
        }).detach();
    }
}
