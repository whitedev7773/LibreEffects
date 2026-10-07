use super::*;

#[derive(Clone, Copy, Debug)]
pub enum LayerSwitch {
    Solo,
    Shy,
    Guide,
}

/// App-local clipboard. Composition references belong to the current project;
/// callers must clear this clipboard when replacing the document.
#[derive(Clone, Debug)]
pub struct LayerClipboard {
    layers: Vec<Layer>,
    composition: CompositionId,
    fps: FrameRate,
    duration: Frame,
}
impl LayerClipboard {
    pub fn len(&self) -> usize {
        self.layers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}
impl Editor {
    pub fn copy_layers(&self, ids: &[LayerId]) -> Result<LayerClipboard, String> {
        let ids: BTreeSet<_> = ids.iter().copied().collect();
        let comp = self.project().composition();
        let layers: Vec<_> = comp
            .layers
            .iter()
            .filter(|l| ids.contains(&l.id))
            .cloned()
            .collect();
        if layers.is_empty() || layers.len() != ids.len() {
            return Err("Select existing layers to copy".into());
        }
        Ok(LayerClipboard {
            layers,
            composition: self.project().composition_id,
            fps: comp.fps,
            duration: comp.duration,
        })
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    match command {
        Command::AddNull => Some((|| {
            let name = format!("Null {}", state.project.next_layer_id);
            super::apply(
                state,
                Command::AddContent {
                    content: Content::Null,
                    width: 100.0,
                    height: 100.0,
                    name,
                },
            )?;
            let layer = state.project.composition.layers.first_mut().unwrap();
            layer.properties.get_mut(&Property::AnchorX).unwrap().value = 0.0;
            layer.properties.get_mut(&Property::AnchorY).unwrap().value = 0.0;
            layer.color = 0xe87979;
            Ok(())
        })()),
        Command::SetLayerSwitch {
            id,
            switch,
            enabled,
        } => Some((|| {
            let layer = state
                .project
                .composition
                .layers
                .iter_mut()
                .find(|l| l.id == *id)
                .ok_or("Layer not found")?;
            if layer.locked {
                return Err("Unlock the layer before editing".into());
            }
            match switch {
                LayerSwitch::Solo => layer.solo = *enabled,
                LayerSwitch::Shy => layer.shy = *enabled,
                LayerSwitch::Guide => layer.guide = *enabled,
            }
            Ok(())
        })()),
        Command::SetHideShy(enabled) => {
            state.project.composition.hide_shy = *enabled;
            Some(Ok(()))
        }
        Command::PasteLayers(clipboard) => Some(paste(state, clipboard)),
        _ => None,
    }
}

fn paste(state: &mut Snapshot, clipboard: &LayerClipboard) -> Result<(), String> {
    if clipboard.layers.is_empty() {
        return Err("Layer clipboard is empty".into());
    }
    let comp = &state.project.composition;
    if clipboard.fps != comp.fps && clipboard.layers.iter().any(Layer::has_opacity_timing) {
        return Err(
            "Pasting native Opacity timing requires matching composition frame rates".into(),
        );
    }
    if clipboard.fps != comp.fps && clipboard.layers.iter().any(Layer::is_three_d) {
        return Err("Pasting 3D layers requires matching composition frame rates".into());
    }
    if clipboard.fps != comp.fps
        && clipboard
            .layers
            .iter()
            .any(|l| l.planar_position().is_some())
    {
        return Err("Pasting joined XY Position requires matching composition frame rates".into());
    }
    let copied: BTreeSet<_> = clipboard.layers.iter().map(|l| l.id).collect();
    for layer in &clipboard.layers {
        if layer.spectrum_sources().any(|source| {
            !copied.contains(&source)
                && (state.project.composition_id != clipboard.composition
                    || comp.layer(source).is_none())
        }) {
            return Err("Copy the Audio Spectrum source as well before pasting into another composition or after source removal".into());
        }
        if layer
            .spatial_position()
            .is_some_and(|position| position.keys.keys().any(|frame| *frame >= comp.duration))
        {
            return Err("Destination duration would lose copied spatial Position keyframes; extend the composition".into());
        }
        if layer
            .planar_position()
            .is_some_and(|p| p.keys.keys().any(|f| *f >= comp.duration))
        {
            return Err("Destination duration would lose copied planar Position keys; extend the composition".into());
        }
        if let Some(matte) = layer.track_matte
            && !copied.contains(&matte.source)
            && (state.project.composition_id != clipboard.composition
                || comp.layer(matte.source).is_none())
        {
            return Err("Copy the matte source as well before pasting into another composition or after source removal".into());
        }
        if let Some(parent) = layer.parent
            && !copied.contains(&parent)
        {
            if state.project.composition_id != clipboard.composition {
                return Err(
                    "Copy the parent hierarchy as well before pasting into another composition"
                        .into(),
                );
            }
            if comp.layer(parent).is_none() {
                return Err("Copied layer's parent was removed; copy its hierarchy again".into());
            }
        }
    }
    let next = state
        .project
        .next_layer_id
        .checked_add(clipboard.layers.len() as u64)
        .filter(|n| *n < u64::MAX)
        .ok_or("Layer ID limit reached")?;
    let mapping: BTreeMap<_, _> = clipboard
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.id, state.project.next_layer_id + i as u64))
        .collect();
    let convert = |frame: Frame| -> Result<Frame, String> {
        u32::try_from(
            clipboard
                .fps
                .convert_frames(u64::from(frame), comp.fps, FrameRounding::Nearest)
                .ok_or("Copied time exceeds supported range")?,
        )
        .map_err(|_| "Copied time exceeds supported range".into())
    };
    let mut layers = clipboard.layers.clone();
    for layer in &mut layers {
        if layer.asset.is_some_and(|id| {
            state.project.asset_library.assets.get(&id).is_none_or(|a| {
                assets::source(&layer.content).as_ref() != Some(&a.content)
                    || a.width() != layer.width
                    || a.height() != layer.height
                    || a.interpretation() != layer.footage_interpretation
            })
        }) {
            layer.asset = None;
        }
        layer.id = mapping[&layer.id];
        layer.parent = layer.parent.map(|p| mapping.get(&p).copied().unwrap_or(p));
        layer.remap_matte(&mapping);
        layer.remap_spectrum_sources(&mapping);
        let end = convert(layer.out_frame(clipboard.duration))?;
        if end > comp.duration {
            return Err(
                "Destination duration would trim copied layers; extend the composition".into(),
            );
        }
        layer.in_frame = convert(layer.in_frame)?;
        if layer.in_frame >= end {
            return Err(
                "Pasted layers are outside the destination duration; extend the composition".into(),
            );
        }
        layer.out_frame = Some(end);
        if let Some(origin) = layer.start_frame {
            layer.start_frame = Some(
                clipboard
                    .fps
                    .convert_origin(origin, comp.fps)
                    .ok_or("Copied layer origin overflow")?,
            );
        }
        layer
            .markers
            .resample(clipboard.fps, comp.fps, comp.duration)?;
        if let Content::ShapeContents(contents) = &mut layer.content {
            contents.map_gradient_frames(|frame| {
                let frame = convert(frame)?;
                if frame >= comp.duration {
                    return Err("Destination duration would lose Gradient Colors keys".into());
                }
                Ok(frame)
            })?;
        }
        for track in layer.all_tracks_mut() {
            let mut keys = BTreeMap::new();
            for (frame, key) in &track.keys {
                let frame = convert(*frame)?;
                if frame >= comp.duration {
                    return Err(
                        "Destination duration would lose copied keyframes; extend the composition"
                            .into(),
                    );
                }
                let mut key = key.clone();
                key.temporal
                    .rescale(clipboard.fps.as_f64() / comp.fps.as_f64());
                if keys.insert(frame, key).is_some() {
                    return Err(
                        "Destination frame rate merges copied keyframes; use a higher frame rate"
                            .into(),
                    );
                }
            }
            track.keys = keys;
        }
        if let Content::Video { start_frame, .. }
        | Content::Audio { start_frame, .. }
        | Content::ImageSequence { start_frame, .. }
        | Content::Composition { start_frame, .. } = &mut layer.content
        {
            *start_frame = clipboard
                .fps
                .convert_origin(*start_frame, comp.fps)
                .ok_or("Copied source time overflow")?;
        }
    }
    let index = state
        .selected
        .and_then(|id| comp.layers.iter().position(|l| l.id == id))
        .unwrap_or(0);
    state.selected = layers.first().map(Layer::id);
    state.project.next_layer_id = next;
    state
        .project
        .composition
        .layers
        .splice(index..index, layers);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switch(e: &mut Editor, id: LayerId, switch: LayerSwitch, enabled: bool) {
        e.execute(Command::SetLayerSwitch {
            id,
            switch,
            enabled,
        })
        .unwrap();
    }
    fn configure(e: &mut Editor, fps: u32, duration: Frame) {
        e.execute(Command::ConfigureComposition {
            name: "Clipboard".into(),
            width: 1920,
            height: 1080,
            fps,
            duration,
        })
        .unwrap();
    }
    #[test]
    fn null_switches_roundtrip_and_undo_with_legacy_defaults() {
        let legacy = Project::default();
        let mut e = Editor::default();
        e.replace_project(Project::from_json(&legacy.to_json().unwrap()).unwrap())
            .unwrap();
        assert!(!e.project().composition().hide_shy());
        e.execute(Command::AddNull).unwrap();
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .property(Property::AnchorX)
                .unwrap()
                .value_at(0),
            0.0
        );
        for s in [LayerSwitch::Solo, LayerSwitch::Shy, LayerSwitch::Guide] {
            switch(&mut e, 1, s, true);
        }
        let before = e.project().clone();
        e.execute(Command::SetHideShy(true)).unwrap();
        let after = e.project().clone();
        assert_eq!(after.version, 11);
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        let mut obsolete: serde_json::Value =
            serde_json::from_str(&after.to_json().unwrap()).unwrap();
        obsolete["version"] = 10.into();
        assert!(Project::from_json(&obsolete.to_string()).is_err());
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
    }
    #[test]
    fn solo_visibility_range_and_guide_policies_are_independent_of_shy() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        switch(&mut e, 1, LayerSwitch::Shy, true);
        e.execute(Command::SetHideShy(true)).unwrap();
        assert!(e.project().composition().layer_active(
            e.project().composition().layer(1).unwrap(),
            0,
            false
        ));
        switch(&mut e, 2, LayerSwitch::Solo, true);
        assert!(
            !e.project()
                .composition()
                .layer_enabled(e.project().composition().layer(1).unwrap(), true)
        );
        switch(&mut e, 2, LayerSwitch::Guide, true);
        let c = e.project().composition();
        assert!(c.layer_active(c.layer(2).unwrap(), 0, true));
        assert!(!c.layer_active(c.layer(2).unwrap(), 0, false));
        assert!(!c.layer_active(c.layer(2).unwrap(), c.duration(), true));
        e.execute(Command::ToggleVisible(2)).unwrap();
        let c = e.project().composition();
        assert!(c.layers().iter().all(|l| !c.layer_active(l, 0, true)));
    }
    #[test]
    fn locked_switch_batch_rolls_back_and_precompose_rejects_changed_semantics() {
        let mut e = Editor::default();
        for _ in 0..2 {
            e.execute(Command::AddRectangle).unwrap();
        }
        e.execute(Command::ToggleLocked(2)).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::Batch(vec![
                Command::SetLayerSwitch {
                    id: 1,
                    switch: LayerSwitch::Solo,
                    enabled: true
                },
                Command::SetLayerSwitch {
                    id: 2,
                    switch: LayerSwitch::Solo,
                    enabled: true
                },
            ]))
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::ToggleLocked(2)).unwrap();
        for s in [LayerSwitch::Solo, LayerSwitch::Guide] {
            switch(&mut e, 1, s, true);
            let before = e.project().clone();
            assert!(
                e.execute(Command::Precompose {
                    layers: vec![1, 2],
                    name: "Source".into()
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
            switch(&mut e, 1, s, false);
        }
    }
    #[test]
    fn clipboard_preserves_stack_hierarchy_assets_properties_and_undo() {
        let mut e = Editor::default();
        e.execute(Command::AddNull).unwrap();
        e.execute(Command::AddContent {
            content: Content::Image { png: "YWJj".into() },
            width: 20.0,
            height: 10.0,
            name: "Image".into(),
        })
        .unwrap();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 2,
            property: Property::PositionX,
            frame: 15,
        })
        .unwrap();
        e.execute(Command::SetEffects {
            id: 2,
            effects: Effects {
                blur: 2.0,
                ..Default::default()
            },
        })
        .unwrap();
        e.execute(Command::SetMask {
            id: 2,
            mask: Some(Mask {
                x: 1.0,
                y: 1.0,
                width: 8.0,
                height: 8.0,
                inverted: false,
            }),
        })
        .unwrap();
        switch(&mut e, 2, LayerSwitch::Shy, true);
        let clipboard = e.copy_layers(&[1, 2, 1]).unwrap();
        assert_eq!(clipboard.len(), 2);
        assert!(!clipboard.is_empty());
        let source = e.project().composition.clone();
        e.execute(Command::NewComposition).unwrap();
        let before = e.project().clone();
        e.execute(Command::PasteLayers(clipboard.clone())).unwrap();
        let c = e.project().composition();
        assert_eq!(
            c.layers().iter().map(Layer::id).collect::<Vec<_>>(),
            vec![3, 4]
        );
        assert_eq!(c.layer(3).unwrap().parent(), Some(4));
        let original = source.layer(2).unwrap();
        let pasted = c.layer(3).unwrap();
        assert_eq!(pasted.properties, original.properties);
        assert_eq!(pasted.effects, original.effects);
        assert_eq!(pasted.mask, original.mask);
        assert_eq!(pasted.transform_offset, original.transform_offset);
        assert!(pasted.shy());
        let (Content::Image { png: a }, Content::Image { png: b }) =
            (&original.content, &pasted.content)
        else {
            panic!("Image content lost")
        };
        assert!(std::sync::Arc::ptr_eq(a, b));
        assert_eq!(e.project().composition_by_id(1).unwrap(), &source);
        let after = e.project().clone();
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        e.execute(Command::PasteLayers(clipboard)).unwrap();
        assert_eq!(
            e.project().composition().layer(5).unwrap().parent(),
            Some(6)
        );
    }
    #[test]
    fn clipboard_external_parent_is_retained_locally_and_rejected_across_compositions() {
        let mut e = Editor::default();
        e.execute(Command::AddNull).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        let clipboard = e.copy_layers(&[2]).unwrap();
        e.execute(Command::PasteLayers(clipboard.clone())).unwrap();
        assert_eq!(e.selected_layer().unwrap().parent(), Some(1));
        e.execute(Command::NewComposition).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::PasteLayers(clipboard.clone()))
                .unwrap_err()
                .contains("parent hierarchy")
        );
        assert_eq!(e.project(), &before);
        e.activate_composition(1).unwrap();
        for id in [2, 3] {
            e.execute(Command::SetParent {
                id,
                parent: None,
                frame: 0,
            })
            .unwrap();
        }
        e.execute(Command::RemoveLayer(1)).unwrap();
        assert!(
            e.execute(Command::PasteLayers(clipboard))
                .unwrap_err()
                .contains("parent was removed")
        );
    }
    #[test]
    fn clipboard_converts_keyframes_ranges_and_negative_source_origins_in_seconds() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                audio: None,
                path: "source.mp4".into(),
                duration: 10.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: VideoPlayback {
                    source_in: 1.0,
                    speed: -0.5,
                },
            },
            width: 10.0,
            height: 10.0,
            name: "Video".into(),
        })
        .unwrap();
        e.execute(Command::TrimLayers {
            ids: vec![1],
            frame: 15,
            start: true,
        })
        .unwrap();
        e.execute(Command::ShiftLayer { id: 1, delta: -15 })
            .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::Rotation,
            frame: 30,
        })
        .unwrap();
        let clipboard = e.copy_layers(&[1]).unwrap();
        let source_time = e
            .selected_layer()
            .unwrap()
            .content()
            .video_source_time(30, 30);
        e.execute(Command::NewComposition).unwrap();
        configure(&mut e, 60, 300);
        e.execute(Command::PasteLayers(clipboard)).unwrap();
        let l = e.selected_layer().unwrap();
        assert_eq!(l.out_frame(300), 270);
        assert!(
            l.property(Property::Rotation)
                .unwrap()
                .keys()
                .contains_key(&60)
        );
        assert_eq!(l.content().video_source_time(60, 60), source_time);
    }
    #[test]
    fn clipboard_rejects_key_collisions_short_duration_and_cycles_without_mutating() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        for frame in [0, 1] {
            e.execute(Command::ToggleKeyframe {
                id: 1,
                property: Property::Rotation,
                frame,
            })
            .unwrap();
        }
        let clipboard = e.copy_layers(&[1]).unwrap();
        e.execute(Command::NewComposition).unwrap();
        configure(&mut e, 10, 50);
        let before = e.project().clone();
        assert!(
            e.execute(Command::PasteLayers(clipboard.clone()))
                .unwrap_err()
                .contains("merges copied keyframes")
        );
        assert_eq!(e.project(), &before);
        configure(&mut e, 30, 100);
        let before = e.project().clone();
        assert!(
            e.execute(Command::PasteLayers(clipboard))
                .unwrap_err()
                .contains("trim copied layers")
        );
        assert_eq!(e.project(), &before);
        e.activate_composition(1).unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 2,
            frame: 0,
        })
        .unwrap();
        let clipboard = e.copy_layers(&[e.selected().unwrap()]).unwrap();
        e.activate_composition(2).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::PasteLayers(clipboard))
                .unwrap_err()
                .contains("Circular")
        );
        assert_eq!(e.project(), &before);
    }
}
