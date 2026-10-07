use super::*;
use libre_effects_core::{AudioParam, Command, Editor, LayerSwitch, TrackEdit};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn metadata(duration: f64) -> AudioMetadata {
    AudioMetadata {
        stream_index: 0,
        sample_rate: SAMPLE_RATE,
        channels: 2,
        channel_layout: "stereo".into(),
        duration,
        start_time: 0.0,
        file_offset: 0.0,
    }
}
fn add_audio(e: &mut Editor, path: &str, duration: f64) -> LayerId {
    e.execute(Command::AddContent {
        content: Content::Audio {
            path: path.into(),
            audio: metadata(duration),
            start_frame: 0,
            playback: Default::default(),
        },
        width: 1.0,
        height: 1.0,
        name: "Synthetic audio".into(),
    })
    .unwrap();
    e.selected().unwrap()
}
fn scene(path: &str, duration: f64) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Selected audio QA".into(),
        width: 64,
        height: 48,
        fps: 30,
        duration: 180,
    })
    .unwrap();
    add_audio(&mut e, path, duration);
    e
}
fn source_file() -> tempfile::NamedTempFile {
    // Only a stamp fixture: no file in these tests contains or decodes media.
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"synthetic source stamp").unwrap();
    file
}
fn ramp_cache(calls: Arc<AtomicUsize>) -> PcmCache {
    PcmCache::with_decoder(Box::new(move |_, _, second, _| {
        calls.fetch_add(1, Ordering::Relaxed);
        Ok((0..SAMPLE_RATE)
            .map(|i| {
                let time = f64::from(second) + f64::from(i) / f64::from(SAMPLE_RATE);
                [(2.0 + time) as f32, (-3.0 - 2.0 * time) as f32]
            })
            .collect())
    }))
}
fn plan(e: &Editor, owner: CompositionId, selected: LayerId) -> SelectedAudioPlan<'_> {
    SelectedAudioPlan::new(
        e.project(),
        owner,
        selected,
        SpectrumInputScope::SelectedLayerOutput,
    )
    .unwrap()
}
fn render(
    plan: &SelectedAudioPlan<'_>,
    cache: &mut PcmCache,
    origin: f64,
    count: usize,
) -> Vec<[f32; 2]> {
    plan.render_window(origin, 0, count, cache, &AtomicBool::new(false))
        .unwrap()
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-6, "{a} != {b}");
}
fn nested_id(e: &Editor, layer: LayerId) -> CompositionId {
    match e.project().composition().layer(layer).unwrap().content() {
        Content::Composition { composition, .. } => *composition,
        _ => panic!("expected precomposition"),
    }
}

#[test]
fn selected_output_excludes_unrelated_master_voices_and_parent_routes() {
    let file = source_file();
    let path = file.path().to_str().unwrap();
    let mut e = scene(path, 3.0);
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Selected bus".into(),
    })
    .unwrap();
    let selected = e.selected().unwrap();
    let root = e.project().active_composition_id();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cache = ramp_cache(calls.clone());
    let expected = render(&plan(&e, root, selected), &mut cache, 0.37, 1500);
    assert!(expected[0][0] > 1.0 && expected[0][1] < -1.0);
    e.execute(Command::Precompose {
        layers: vec![selected],
        name: "Muted outer route".into(),
    })
    .unwrap();
    let outer = e.selected().unwrap();
    let owner = nested_id(&e, outer);
    e.execute(Command::SetAudioEnabled {
        id: outer,
        enabled: false,
    })
    .unwrap();
    e.execute(Command::EditAudio {
        id: outer,
        parameter: AudioParam::Pan,
        edit: TrackEdit::Value {
            frame: 0,
            value: 100.0,
        },
    })
    .unwrap();
    let unrelated = add_audio(&mut e, "unrelated-offline-player.wav", 3.0);
    e.execute(Command::SetLayerSwitch {
        id: unrelated,
        switch: LayerSwitch::Solo,
        enabled: true,
    })
    .unwrap();
    let selected_plan = plan(&e, owner, selected);
    assert_eq!(selected_plan.dependencies().collect::<Vec<_>>(), [path]);
    assert_eq!(render(&selected_plan, &mut cache, 0.37, 1500), expected);
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "plans share complete PCM chunks"
    );
    assert!(
        SelectedAudioPlan::new(
            e.project(),
            root,
            selected,
            SpectrumInputScope::SelectedLayerOutput
        )
        .is_err()
    );
}

#[test]
fn selected_origin_trim_gain_pan_and_nested_clock_stay_continuous() {
    let file = source_file();
    let mut e = scene(file.path().to_str().unwrap(), 3.0);
    e.execute(Command::SetVideoSourceIn {
        id: 1,
        seconds: 0.25,
    })
    .unwrap();
    e.execute(Command::SetVideoSpeed { id: 1, speed: 1.5 })
        .unwrap();
    e.execute(Command::EditAudio {
        id: 1,
        parameter: AudioParam::RightLevel,
        edit: TrackEdit::Value {
            frame: 0,
            value: -6.020599913279624,
        },
    })
    .unwrap();
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Timed bus".into(),
    })
    .unwrap();
    let selected = e.selected().unwrap();
    let owner = e.project().active_composition_id();
    e.execute(Command::SetLayerStart {
        id: selected,
        frame: 15,
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: selected,
        start: 30,
        end: 60,
    })
    .unwrap();
    e.execute(Command::EditAudio {
        id: selected,
        parameter: AudioParam::Pan,
        edit: TrackEdit::Value {
            frame: 0,
            value: 100.0,
        },
    })
    .unwrap();
    e.execute(Command::ToggleVisible(selected)).unwrap();
    let mut cache = ramp_cache(Arc::new(AtomicUsize::new(0)));
    let origin = 1.0 - 0.5 / 48_000.0;
    let samples = render(&plan(&e, owner, selected), &mut cache, origin, 3);
    assert_eq!(samples[0], [0.0; 2]);
    for (index, sample) in samples.iter().enumerate().skip(1) {
        let owner_time = origin + index as f64 / 48_000.0;
        let source_time = 0.25 + (owner_time - 0.5) * 1.5;
        near(sample[0], 0.0);
        near(
            sample[1],
            ((2.0 + source_time) + (-3.0 - 2.0 * source_time) * 0.5) as f32,
        );
    }
    assert_eq!(
        render(&plan(&e, owner, selected), &mut cache, 2.0, 1),
        [[0.0; 2]]
    );
    // Alter the chosen precomposition clock, not the visual sampling grid.
    e.execute(Command::SetTimeRemap {
        id: selected,
        enabled: true,
    })
    .unwrap();
    e.execute(Command::EditTimeRemap {
        id: selected,
        edit: TrackEdit::Value {
            frame: 30,
            value: 0.4,
        },
    })
    .unwrap();
    e.execute(Command::EditTimeRemap {
        id: selected,
        edit: TrackEdit::Value {
            frame: 59,
            value: 1.0,
        },
    })
    .unwrap();
    e.execute(Command::EditAudio {
        id: selected,
        parameter: AudioParam::Pan,
        edit: TrackEdit::Value {
            frame: 0,
            value: 0.0,
        },
    })
    .unwrap();
    let samples = render(&plan(&e, owner, selected), &mut cache, 1.012345, 3);
    for (index, sample) in samples.iter().enumerate() {
        let frame = (1.012345 + index as f64 / 48_000.0) * 30.0;
        let child_time = 0.4 + (frame - 30.0) / 29.0 * 0.6;
        let source_time = 0.25 + 1.5 * child_time;
        near(sample[0], (2.0 + source_time) as f32);
        near(sample[1], ((-3.0 - 2.0 * source_time) * 0.5) as f32);
    }
    assert_ne!(
        samples[0], samples[1],
        "audio must not quantize to a visual frame"
    );
}

#[test]
fn selected_windows_are_partition_and_seek_order_independent_with_fractional_origins() {
    let file = source_file();
    let mut e = scene(file.path().to_str().unwrap(), 3.0);
    e.execute(Command::SetVideoSpeed { id: 1, speed: 1.5 })
        .unwrap();
    let selected_plan = plan(&e, e.project().active_composition_id(), 1);
    let mut cache = ramp_cache(Arc::new(AtomicUsize::new(0)));
    let origin = 0.666_66 + 0.375 / 48_000.0;
    let whole = render(&selected_plan, &mut cache, origin, 48_000);
    let mut partitioned = vec![[0.0; 2]; whole.len()];
    let starts: Vec<_> = (0..whole.len()).step_by(157).collect();
    for start in starts.into_iter().rev() {
        let count = 157.min(whole.len() - start);
        let chunk = selected_plan
            .render_window(
                origin,
                start as u64,
                count,
                &mut cache,
                &AtomicBool::new(false),
            )
            .unwrap();
        partitioned[start..start + count].copy_from_slice(&chunk);
    }
    assert_eq!(whole, partitioned);
    near(whole[0][0], (2.0 + origin * 1.5) as f32);
}

#[test]
fn source_length_and_layer_boundaries_zero_pad_without_extending_pcm() {
    let file = source_file();
    // Exact source end is inside an ordinary visual frame.
    let duration = 48_013.0 / 48_000.0;
    let mut e = scene(file.path().to_str().unwrap(), duration);
    let owner = e.project().active_composition_id();
    let mut cache = ramp_cache(Arc::new(AtomicUsize::new(0)));
    assert_eq!(
        render(&plan(&e, owner, 1), &mut cache, -2.0 / 48_000.0, 3)[..2],
        [[0.0; 2]; 2]
    );
    let tail = render(
        &plan(&e, owner, 1),
        &mut cache,
        duration - 1.5 / 48_000.0,
        4,
    );
    near(tail[0][0], (2.0 + duration - 1.5 / 48_000.0) as f32);
    near(tail[1][0], ((2.0 + duration - 1.0 / 48_000.0) * 0.5) as f32);
    assert_eq!(tail[2..], [[0.0; 2]; 2]);
    e.execute(Command::FreezeVideo { id: 1, frame: 15 })
        .unwrap();
    assert!(
        render(&plan(&e, owner, 1), &mut cache, 0.2, 100)
            .iter()
            .all(|v| *v == [0.0; 2])
    );
}

#[test]
fn descendant_switches_guide_and_solo_control_audio_but_preserve_dependencies() {
    let first = source_file();
    let second = source_file();
    let mut e = scene(first.path().to_str().unwrap(), 3.0);
    let other = add_audio(&mut e, second.path().to_str().unwrap(), 3.0);
    e.execute(Command::Precompose {
        layers: vec![1, other],
        name: "Two voices".into(),
    })
    .unwrap();
    let selected = e.selected().unwrap();
    let owner = e.project().active_composition_id();
    let child = nested_id(&e, selected);
    let mut cache = ramp_cache(Arc::new(AtomicUsize::new(0)));
    near(
        render(&plan(&e, owner, selected), &mut cache, 0.5, 1)[0][0],
        5.0,
    );
    e.activate_composition(child).unwrap();
    e.execute(Command::SetLayerSwitch {
        id: other,
        switch: LayerSwitch::Guide,
        enabled: true,
    })
    .unwrap();
    near(
        render(&plan(&e, owner, selected), &mut cache, 0.5, 1)[0][0],
        2.5,
    );
    e.execute(Command::SetLayerSwitch {
        id: other,
        switch: LayerSwitch::Guide,
        enabled: false,
    })
    .unwrap();
    e.execute(Command::SetLayerSwitch {
        id: other,
        switch: LayerSwitch::Solo,
        enabled: true,
    })
    .unwrap();
    near(
        render(&plan(&e, owner, selected), &mut cache, 0.5, 1)[0][0],
        2.5,
    );
    e.execute(Command::SetAudioEnabled {
        id: other,
        enabled: false,
    })
    .unwrap();
    assert_eq!(
        render(&plan(&e, owner, selected), &mut cache, 0.5, 1),
        [[0.0; 2]]
    );
    assert_eq!(plan(&e, owner, selected).dependencies().count(), 2);
    std::fs::remove_file(first.path()).unwrap();
    assert!(
        plan(&e, owner, selected)
            .render_window(0.5, 0, 1, &mut cache, &AtomicBool::new(false))
            .unwrap_err()
            .contains("Audio offline")
    );
}

#[test]
fn every_request_rechecks_pinned_sources_even_with_cached_or_silent_windows() {
    let file = source_file();
    let e = scene(file.path().to_str().unwrap(), 3.0);
    let selected_plan = plan(&e, e.project().active_composition_id(), 1);
    let mut cache = ramp_cache(Arc::new(AtomicUsize::new(0)));
    render(&selected_plan, &mut cache, 0.2, 1);
    std::fs::write(file.path(), b"changed file with a different length").unwrap();
    assert!(
        selected_plan
            .render_window(0.2, 0, 1, &mut cache, &AtomicBool::new(false))
            .unwrap_err()
            .contains("source changed")
    );
    assert!(
        selected_plan
            .validate_sources(&mut cache, &AtomicBool::new(false))
            .unwrap_err()
            .contains("source changed")
    );
    assert!(
        selected_plan
            .render_window(40.0, 0, 1, &mut cache, &AtomicBool::new(false))
            .unwrap_err()
            .contains("source changed")
    );
    std::fs::remove_file(file.path()).unwrap();
    assert!(
        selected_plan
            .render_window(0.2, 0, 1, &mut cache, &AtomicBool::new(false))
            .unwrap_err()
            .contains("Audio offline")
    );
}

#[test]
fn cancellation_and_nonfinite_decoder_results_are_retryable_and_never_cached() {
    let file = source_file();
    let e = scene(file.path().to_str().unwrap(), 3.0);
    let selected_plan = plan(&e, e.project().active_composition_id(), 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let decoder_calls = calls.clone();
    let mut cache = PcmCache::with_decoder(Box::new(move |_, _, _, cancel| {
        match decoder_calls.fetch_add(1, Ordering::Relaxed) {
            0 => {
                cancel.store(true, Ordering::Relaxed);
                Ok(vec![[5.0; 2]; 48_000])
            }
            1 => Ok(vec![[f32::NAN, 0.0]; 48_000]),
            _ => Ok(vec![[5.0; 2]; 48_000]),
        }
    }));
    let cancel = AtomicBool::new(true);
    assert!(
        selected_plan
            .render_window(0.0, 0, 1, &mut cache, &cancel)
            .unwrap_err()
            .contains("canceled")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    cancel.store(false, Ordering::Relaxed);
    assert!(
        selected_plan
            .render_window(0.0, 0, 1, &mut cache, &cancel)
            .unwrap_err()
            .contains("canceled")
    );
    cancel.store(false, Ordering::Relaxed);
    assert!(
        selected_plan
            .render_window(0.0, 0, 1, &mut cache, &cancel)
            .unwrap_err()
            .contains("non-finite")
    );
    assert_eq!(
        selected_plan
            .render_window(0.0, 0, 1, &mut cache, &cancel)
            .unwrap(),
        [[5.0; 2]]
    );
    assert_eq!(calls.load(Ordering::Relaxed), 3);
}

#[test]
fn invalid_windows_fail_before_decode_and_invalid_selected_targets_fail() {
    let file = source_file();
    let mut e = scene(file.path().to_str().unwrap(), 3.0);
    let owner = e.project().active_composition_id();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cache = ramp_cache(calls.clone());
    for (origin, start, count) in [
        (f64::NAN, 0, 1),
        (f64::INFINITY, 0, 1),
        (0.0, 0, 48_001),
        (0.0, u64::MAX, 1),
        (0.0, 1_u64 << 53, 1),
    ] {
        assert!(
            plan(&e, owner, 1)
                .render_window(origin, start, count, &mut cache, &AtomicBool::new(false))
                .is_err()
        );
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    e.execute(Command::AddRectangle).unwrap();
    assert!(
        SelectedAudioPlan::new(
            e.project(),
            owner,
            e.selected().unwrap(),
            SpectrumInputScope::SelectedLayerOutput
        )
        .is_err()
    );
    assert!(
        SelectedAudioPlan::new(
            e.project(),
            owner + 999,
            1,
            SpectrumInputScope::SelectedLayerOutput
        )
        .is_err()
    );
}

#[test]
fn selected_audio_graph_limits_bound_repeated_composition_instances() {
    let file = source_file();
    let mut e = scene(file.path().to_str().unwrap(), 3.0);
    for _ in 0..12 {
        let child = e.project().active_composition_id();
        e.execute(Command::NewComposition).unwrap();
        for _ in 0..2 {
            e.execute(Command::AddCompositionLayer {
                composition: child,
                frame: 0,
            })
            .unwrap();
        }
    }
    let result = SelectedAudioPlan::new(
        e.project(),
        e.project().active_composition_id(),
        e.selected().unwrap(),
        SpectrumInputScope::SelectedLayerOutput,
    );
    assert!(matches!(result, Err(error) if error.contains("4096")));
}

#[test]
fn pcm_work_estimate_counts_voice_steps_and_silent_structural_dependencies() {
    // Work admission does not open the media, even for an offline source.
    let mut e = scene("offline-estimate-only.wav", 3.0);
    let other = add_audio(&mut e, "offline-estimate-only.wav", 3.0);
    e.execute(Command::AddRectangle).unwrap();
    let visual = e.selected().unwrap();
    e.execute(Command::Precompose {
        layers: vec![1, other, visual],
        name: "Estimated subtree".into(),
    })
    .unwrap();
    let selected = e.selected().unwrap();
    let owner = e.project().active_composition_id();
    let estimate = plan(&e, owner, selected).work_estimate(10).unwrap();
    assert_eq!(
        estimate,
        PcmWorkEstimate {
            sample_work: 120,
            graph_nodes: 4,
            dependencies: 1,
            total_work: 126,
        }
    );
    assert_eq!(
        plan(&e, owner, selected)
            .work_estimate(20)
            .unwrap()
            .sample_work,
        240
    );
    for count in [48_001, usize::MAX] {
        assert!(plan(&e, owner, selected).work_estimate(count).is_err());
    }
    e.execute(Command::SetAudioEnabled {
        id: selected,
        enabled: false,
    })
    .unwrap();
    assert_eq!(
        plan(&e, owner, selected).work_estimate(48_000).unwrap(),
        PcmWorkEstimate {
            sample_work: 0,
            graph_nodes: 4,
            dependencies: 1,
            total_work: 6,
        }
    );
}
