use super::*;

impl Content {
    /// Integer time conversion: sample the preceding source frame, without drift.
    pub fn composition_frame(
        &self,
        frame: Frame,
        parent_fps: impl Into<FrameRate>,
        source: &Composition,
    ) -> Option<Frame> {
        let Self::Composition { start_frame, .. } = self else {
            return None;
        };
        let elapsed = i64::from(frame).checked_sub(*start_frame)?;
        if elapsed < 0 {
            return None;
        }
        let source_frame =
            parent_fps
                .into()
                .convert_frames(elapsed as u64, source.fps, FrameRounding::Floor)?;
        (source_frame < u64::from(source.duration)).then_some(source_frame as Frame)
    }
}

pub(super) fn validate(project: &Project) -> Result<(), String> {
    fn depth(
        project: &Project,
        id: CompositionId,
        path: &mut BTreeSet<CompositionId>,
        known: &mut BTreeMap<CompositionId, usize>,
    ) -> Result<usize, String> {
        if let Some(depth) = known.get(&id) {
            return Ok(*depth);
        }
        if !path.insert(id) {
            return Err("Circular composition reference".into());
        }
        let comp = project
            .composition_by_id(id)
            .ok_or("Missing source composition")?;
        let mut result = 1;
        for layer in &comp.layers {
            if let Content::Composition { composition, .. } = layer.content {
                if project.version < 10 {
                    return Err("Nested compositions require project version 10".into());
                }
                result = result.max(1 + depth(project, composition, path, known)?);
                if result > 16 {
                    return Err("Composition nesting is limited to 16 levels".into());
                }
            }
        }
        path.remove(&id);
        known.insert(id, result);
        Ok(result)
    }
    let mut known = BTreeMap::new();
    for (id, _) in project.compositions() {
        depth(project, id, &mut BTreeSet::new(), &mut known)?;
    }
    Ok(())
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    match command {
        Command::AddCompositionLayer { composition, frame } => {
            Some(add_layer(state, *composition, *frame))
        }
        Command::Precompose { layers, name } => Some(precompose(state, layers, name)),
        _ => None,
    }
}

fn add_layer(state: &mut Snapshot, composition: CompositionId, frame: Frame) -> Result<(), String> {
    let source = state
        .project
        .composition_by_id(composition)
        .ok_or("Source composition not found")?;
    let (name, width, height, fps, duration) = (
        source.name.clone(),
        source.width,
        source.height,
        source.fps,
        source.duration,
    );
    let parent = &state.project.composition;
    if frame >= parent.duration {
        return Err("Place the composition inside the timeline".into());
    }
    let end = (u64::from(frame)
        + fps
            .convert_frames(u64::from(duration), parent.fps, FrameRounding::Ceil)
            .ok_or("Nested composition duration overflow")?)
    .min(u64::from(parent.duration)) as Frame;
    super::apply(
        state,
        Command::AddContent {
            content: Content::Composition {
                composition,
                start_frame: i64::from(frame),
            },
            width: f64::from(width),
            height: f64::from(height),
            name,
        },
    )?;
    let layer = state.project.composition.layers.first_mut().unwrap();
    layer.in_frame = frame;
    layer.out_frame = Some(end);
    Ok(())
}

fn precompose(state: &mut Snapshot, layers: &[LayerId], name: &str) -> Result<(), String> {
    let ids: BTreeSet<_> = layers.iter().copied().collect();
    if ids.is_empty() {
        return Err("Select layers to pre-compose".into());
    }
    if name.trim().is_empty() || name.len() > 1024 {
        return Err("Enter a valid composition name".into());
    }
    if state.project.other_compositions.len() >= 99
        || state.project.next_composition_id >= u64::MAX - 1
    {
        return Err("Composition limit reached".into());
    }
    let parent = &state.project.composition;
    if parent.layers.iter().any(|l| l.solo()) {
        return Err("Clear Solo switches before pre-composing to preserve the composite".into());
    }
    let positions: Vec<_> = parent
        .layers
        .iter()
        .enumerate()
        .filter(|(_, l)| ids.contains(&l.id))
        .map(|(i, _)| i)
        .collect();
    if positions.len() != ids.len() {
        return Err("Selected layer no longer exists".into());
    }
    let first = positions[0];
    if parent.layers.iter().any(|l| {
        ids.contains(&l.id)
            && (matches!(l.content, Content::Adjustment) || !l.blend_mode.is_normal())
    }) && parent.layers[first..].iter().any(|l| !ids.contains(&l.id))
    {
        return Err("Include all layers below adjustment or blended layers when pre-composing to preserve their input".into());
    }
    if positions.last().unwrap() - first + 1 != ids.len() {
        return Err(
            "Select consecutive layers to preserve stacking order when pre-composing".into(),
        );
    }
    for layer in &parent.layers {
        let selected = ids.contains(&layer.id);
        if selected && layer.guide() {
            return Err("Turn off Guide on selected layers before pre-composing; nested guides are excluded".into());
        }
        if selected && layer.locked {
            return Err("Unlock selected layers before pre-composing".into());
        }
        if layer.parent.is_some_and(|id| ids.contains(&id) != selected) {
            return Err("Include the complete parent hierarchy when pre-composing".into());
        }
    }
    let mut source = parent.clone();
    source.markers = Default::default();
    source.name = name.trim().into();
    source.layers = parent.layers[first..first + ids.len()].to_vec();
    let start = source.layers.iter().map(|l| l.in_frame).min().unwrap();
    let end = source
        .layers
        .iter()
        .map(|l| l.out_frame(parent.duration))
        .max()
        .unwrap();
    let id = state.project.next_composition_id;
    state.project.next_composition_id += 1;
    state
        .project
        .composition
        .layers
        .drain(first..first + ids.len());
    state.project.other_compositions.insert(id, source);
    add_layer(state, id, 0)?;
    let mut replacement = state.project.composition.layers.remove(0);
    replacement.in_frame = start;
    replacement.out_frame = Some(end);
    // Same canvas and timeline keep every keyframe, mask, parent and transform intact.
    state.project.composition.layers.insert(first, replacement);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excessive_nesting_is_rejected_atomically() {
        let mut e = Editor::default();
        for source in 1..16 {
            e.execute(Command::NewComposition).unwrap();
            e.execute(Command::AddCompositionLayer {
                composition: source,
                frame: 0,
            })
            .unwrap();
        }
        e.execute(Command::NewComposition).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::AddCompositionLayer {
                composition: 16,
                frame: 0
            })
            .unwrap_err()
            .contains("16 levels")
        );
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn precompose_moves_layers_keeps_keys_and_parents_and_is_one_undo() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::PositionX,
            frame: 10,
        })
        .unwrap();
        let before = e.project().clone();
        e.execute(Command::Precompose {
            layers: vec![1, 2],
            name: "Title".into(),
        })
        .unwrap();
        assert_eq!(e.project().composition.layers.len(), 1);
        assert_eq!(
            e.project().composition_by_id(2).unwrap().layers,
            before.composition.layers
        );
        let after = e.project().clone();
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        e.activate_composition(2).unwrap();
        assert!(e.execute(Command::DeleteComposition).is_err());
        let saved = e.project().clone();
        assert!(
            e.execute(Command::AddCompositionLayer {
                composition: 1,
                frame: 0
            })
            .unwrap_err()
            .contains("Circular")
        );
        assert!(
            e.execute(Command::AddCompositionLayer {
                composition: 999,
                frame: 0
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
    }
    #[test]
    fn precompose_rejects_stack_gaps_and_external_parents_atomically() {
        let mut e = Editor::default();
        for _ in 0..3 {
            e.execute(Command::AddRectangle).unwrap();
        }
        let before = e.project().clone();
        assert!(
            e.execute(Command::Precompose {
                layers: vec![1, 3],
                name: "Gap".into()
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        let before = e.project().clone();
        for layers in [vec![1], vec![2]] {
            assert!(
                e.execute(Command::Precompose {
                    layers,
                    name: "Parent".into()
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
    }
    #[test]
    fn nested_time_uses_source_fps_and_shift_keeps_source_origin() {
        let mut e = Editor::default();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "24 fps".into(),
            width: 100,
            height: 100,
            fps: 24,
            duration: 24,
        })
        .unwrap();
        let source = e.project().composition().clone();
        e.activate_composition(1).unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 2,
            frame: 10,
        })
        .unwrap();
        let layer = e.selected_layer().unwrap();
        assert_eq!((layer.in_frame(), layer.out_frame(150)), (10, 40));
        assert_eq!(layer.content().composition_frame(9, 30, &source), None);
        assert_eq!(layer.content().composition_frame(39, 30, &source), Some(23));
        assert_eq!(layer.content().composition_frame(40, 30, &source), None);
        e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .content()
                .composition_frame(44, 30, &source),
            Some(23)
        );
    }
}
