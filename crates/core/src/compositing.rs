//! Explicit, versioned normal-compositing arithmetic. Never inferred from assets.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositingProfile {
    #[default]
    NativeV1,
    /// Signed byte opacity interpolation, restricted to opaque source and target
    /// pixels. Partial-alpha pixels retain the native compositing contract.
    OpaqueOpacityByte257V1,
}

impl CompositingProfile {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::NativeV1
    }
}

impl Composition {
    pub fn compositing_profile(&self) -> CompositingProfile {
        self.compositing_profile
    }
}

pub(super) fn materialized(project: &Project) -> bool {
    project
        .compositions()
        .iter()
        .any(|(_, comp)| !comp.compositing_profile.is_default())
}

pub(super) fn edits_only(command: &Command, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match command {
        Command::SetCompositingProfile { .. } => true,
        Command::Batch(commands) => {
            !commands.is_empty()
                && commands.len() <= 10_000
                && commands.iter().all(|c| edits_only(c, depth + 1))
        }
        _ => false,
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::SetCompositingProfile {
        composition,
        profile,
    } = command
    else {
        return None;
    };
    Some((|| {
        let project = &mut state.project;
        let comp = if *composition == project.composition_id {
            &mut project.composition
        } else {
            project
                .other_compositions
                .get_mut(composition)
                .ok_or("Composition not found")?
        };
        comp.compositing_profile = *profile;
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compositing_profile_edits_preserve_sources_and_history() {
        let mut editor = Editor::default();
        let before = editor.current.clone();
        let json = serde_json::to_value(editor.project()).unwrap();
        assert!(json["composition"].get("compositing_profile").is_none());
        editor
            .execute(Command::SetCompositingProfile {
                composition: 1,
                profile: CompositingProfile::OpaqueOpacityByte257V1,
            })
            .unwrap();
        let changed = editor.project().clone();
        assert_eq!(changed.version, 84);
        let mut expected = before.project.clone();
        expected.version = 84;
        expected.composition.compositing_profile = CompositingProfile::OpaqueOpacityByte257V1;
        assert_eq!(changed, expected);
        assert_eq!(
            Project::from_json(&changed.to_json().unwrap()).unwrap(),
            changed
        );
        let bytes = project_file::encode(&changed, None).unwrap();
        assert_eq!(project_file::decode(&bytes).unwrap().project, changed);
        editor
            .execute(Command::SetCompositingProfile {
                composition: 1,
                profile: CompositingProfile::OpaqueOpacityByte257V1,
            })
            .unwrap();
        editor.undo();
        assert_eq!(editor.current, before);
        editor.redo();
        assert_eq!(editor.project(), &changed);
        assert!(
            editor
                .execute(Command::Batch(vec![
                    Command::SetCompositingProfile {
                        composition: 1,
                        profile: CompositingProfile::NativeV1
                    },
                    Command::SetCompositingProfile {
                        composition: 999,
                        profile: CompositingProfile::NativeV1
                    }
                ]))
                .is_err()
        );
        assert_eq!(editor.project(), &changed);
        let mut invalid = serde_json::to_value(&changed).unwrap();
        invalid["version"] = 83.into();
        assert!(
            Project::from_json(&invalid.to_string())
                .unwrap_err()
                .contains("84")
        );
        invalid["version"] = 84.into();
        invalid["composition"]["compositing_profile"] = "Guess".into();
        assert!(Project::from_json(&invalid.to_string()).is_err());
        editor
            .execute(Command::SetCompositingProfile {
                composition: 1,
                profile: CompositingProfile::NativeV1,
            })
            .unwrap();
        assert_eq!(editor.project().version, 84);
        assert_eq!(
            editor.project().composition().compositing_profile(),
            CompositingProfile::NativeV1
        );
    }
    #[test]
    fn compositing_profile_automation_is_atomic_and_schema_gated() {
        let mut project = Project::default();
        project
            .apply_automation_command(
                1,
                Command::SetCompositingProfile {
                    composition: 1,
                    profile: CompositingProfile::OpaqueOpacityByte257V1,
                },
            )
            .unwrap();
        assert_eq!(project.version, 84);
        let changed = project.clone();
        assert!(
            project
                .apply_automation_command(
                    1,
                    Command::SetCompositingProfile {
                        composition: 999,
                        profile: CompositingProfile::NativeV1
                    }
                )
                .is_err()
        );
        assert_eq!(project, changed);
        project.version = 83;
        let invalid = project.clone();
        assert!(
            project
                .apply_automation_command(
                    1,
                    Command::SetCompositingProfile {
                        composition: 1,
                        profile: CompositingProfile::NativeV1
                    }
                )
                .is_err()
        );
        assert_eq!(project, invalid);
    }
}
