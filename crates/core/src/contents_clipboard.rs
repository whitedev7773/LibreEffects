//! Session-local, exact source snapshots for sibling Contents copy/cut/paste.
use super::*;

#[cfg(test)]
#[path = "contents_clipboard_tests.rs"]
mod tests;

/// Immutable app-local Contents clipboard. Callers must clear it when replacing
/// the document. Frames are composition frames and are never retimed: paste is
/// restricted to the captured composition identity and exact frame rate.
/// Node identities are layer-local: destination allocations may numerically match
/// nodes in another layer. Gradient stops and stored path poses keep local IDs.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentsClipboard {
    contents: ShapeContents,
    composition: CompositionId,
    fps: FrameRate,
    source_version: u32,
    required_version: u32,
}
impl ContentsClipboard {
    pub fn len(&self) -> usize {
        self.contents.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.contents.items.is_empty()
    }
}

impl Editor {
    /// Copy a nonempty exact set of immediate siblings in source order, including
    /// full nested payloads. Does not affect selection, allocators or history.
    pub fn copy_contents(
        &self,
        id: LayerId,
        parent: u64,
        items: &[u64],
    ) -> Result<ContentsClipboard, String> {
        self.project().validate()?;
        document::validate_budget(self.project())?;
        let layer = self
            .project()
            .composition
            .layer(id)
            .ok_or("Layer not found")?;
        if layer.locked {
            return Err("Unlock the layer before copying Contents".into());
        }
        let Content::ShapeContents(contents) = &layer.content else {
            return Err("Select a Contents shape layer".into());
        };
        let nodes = exact_siblings(contents, parent, items)?
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let required_version = required_version(&nodes);
        Ok(ContentsClipboard {
            contents: ShapeContents {
                items: nodes,
                next_id: contents.next_id,
            },
            composition: self.project().composition_id,
            fps: self.project().composition.fps,
            source_version: self.project().version,
            required_version,
        })
    }

    /// Execute one atomic paste and return the inserted root IDs in source order.
    /// Hosts with an existing command pipeline can dispatch `ContentsEdit::Paste`
    /// instead and inspect the destination's newly inserted children afterward.
    pub fn paste_contents(
        &mut self,
        id: LayerId,
        parent: u64,
        index: usize,
        clipboard: &ContentsClipboard,
    ) -> Result<Vec<u64>, String> {
        self.execute(Command::Contents {
            id,
            edit: ContentsEdit::Paste {
                parent,
                index,
                clipboard: clipboard.clone(),
            },
        })?;
        let Content::ShapeContents(contents) = &self
            .project()
            .composition
            .layer(id)
            .expect("successful paste retains its layer")
            .content
        else {
            unreachable!("successful paste retains Contents")
        };
        Ok(siblings(contents, parent)?[index..index + clipboard.len()]
            .iter()
            .map(|node| node.id)
            .collect())
    }
}

fn siblings(contents: &ShapeContents, parent: u64) -> Result<&[ContentsNode], String> {
    if parent == 0 {
        return Ok(&contents.items);
    }
    match &contents
        .node(parent)
        .ok_or("Contents group no longer exists")?
        .kind
    {
        ContentsKind::Group(children) => Ok(children),
        _ => Err("Choose a Contents group".into()),
    }
}

fn exact_siblings<'a>(
    contents: &'a ShapeContents,
    parent: u64,
    items: &[u64],
) -> Result<Vec<&'a ContentsNode>, String> {
    let selected = items.iter().copied().collect::<BTreeSet<_>>();
    if selected.is_empty() || selected.len() != items.len() {
        return Err("Choose nonempty, distinct Contents siblings".into());
    }
    let nodes: Vec<_> = siblings(contents, parent)?
        .iter()
        .filter(|node| selected.contains(&node.id))
        .collect();
    if nodes.len() != items.len() {
        return Err("Choose immediate siblings of the Contents group".into());
    }
    Ok(nodes)
}

/// Only the stored subtree's existing feature gates may promote the destination.
/// This deliberately does not materialize defaults or run project migrations.
fn required_version(nodes: &[ContentsNode]) -> u32 {
    nodes.iter().fold(43, |version, node| {
        let mut required = 43;
        if node.parameters.contains_key(&ContentsParam::Skew)
            || node.parameters.contains_key(&ContentsParam::SkewAxis)
        {
            required = 44;
        }
        if let Some(gradient) = node.kind.gradient() {
            required = gradient
                .colors_animation()
                .map_or(45, |animation| animation.required_version());
        }
        if node.composite != PaintComposite::BelowPrevious {
            required = required.max(46);
        }
        if node.blend != PaintBlend::Normal {
            required = required.max(47);
        }
        if matches!(node.kind, ContentsKind::TrimPaths) {
            required = required.max(50);
        }
        if let ContentsKind::Group(children) = &node.kind {
            required = required.max(required_version(children));
        }
        version.max(required)
    })
}

/// Dedicated clipboard edits may be batched together, but must never fall through
/// a mixed batch into generic schema/asset migration. Bound these new batches
/// before the existing recursive command handlers or candidate clone run.
pub(super) fn route(command: &Command) -> Result<bool, String> {
    let mut stack = vec![(command, 0usize)];
    let (mut found, mut pure, mut count, mut max_depth) = (false, true, 0usize, 0usize);
    while let Some((command, depth)) = stack.pop() {
        count = count.saturating_add(1);
        max_depth = max_depth.max(depth);
        match command {
            Command::Contents {
                edit: ContentsEdit::RemoveSiblings { .. } | ContentsEdit::Paste { .. },
                ..
            } => found = true,
            Command::Batch(commands) if !commands.is_empty() => {
                stack.extend(commands.iter().map(|command| (command, depth + 1)));
            }
            _ => pure = false,
        }
    }
    if found && (!pure || count > 10000 || max_depth > 64) {
        return Err(
            "Contents clipboard edits require a bounded, nonempty clipboard-only transaction"
                .into(),
        );
    }
    Ok(found)
}

pub(super) fn apply(state: &mut Snapshot, id: LayerId, edit: &ContentsEdit) -> Result<(), String> {
    match edit {
        ContentsEdit::RemoveSiblings { parent, items } => {
            let layer = editing::editable(state, id)?;
            let Content::ShapeContents(contents) = &mut layer.content else {
                return Err("Select a Contents shape layer".into());
            };
            let selected = exact_siblings(contents, *parent, items)?
                .into_iter()
                .map(|node| node.id)
                .collect::<BTreeSet<_>>();
            contents
                .group_mut(*parent)?
                .retain(|node| !selected.contains(&node.id));
        }
        ContentsEdit::Paste {
            parent,
            index,
            clipboard,
        } => {
            if clipboard.is_empty() {
                return Err("Contents clipboard is empty".into());
            }
            if state.project.composition_id != clipboard.composition
                || state.project.composition.fps != clipboard.fps
            {
                return Err("Paste Contents in the same composition and frame rate as Copy".into());
            }
            clipboard
                .contents
                .validate_version(state.project.composition.duration, clipboard.source_version)?;
            let layer = editing::editable(state, id)?;
            let Content::ShapeContents(contents) = &mut layer.content else {
                return Err("Select a Contents shape layer".into());
            };
            if *index > siblings(contents, *parent)?.len() {
                return Err("Contents order is outside the group".into());
            }
            fn rekey(
                nodes: &mut [ContentsNode],
                contents: &mut ShapeContents,
            ) -> Result<(), String> {
                for node in nodes {
                    node.id = contents.allocate()?;
                    if let ContentsKind::Group(children) = &mut node.kind {
                        rekey(children, contents)?;
                    }
                }
                Ok(())
            }
            let mut nodes = clipboard.contents.items.clone();
            rekey(&mut nodes, contents)?;
            contents.group_mut(*parent)?.splice(*index..*index, nodes);
            state.project.version = state.project.version.max(clipboard.required_version);
        }
        _ => unreachable!("clipboard route only accepts clipboard edits"),
    }
    Ok(())
}
