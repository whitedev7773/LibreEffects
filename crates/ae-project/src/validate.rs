use crate::*;
use std::collections::{BTreeMap, BTreeSet};

/// Owned validated data. No mutable accessor: edits must be revalidated.
#[derive(Clone, Debug)]
pub struct ValidatedProject {
    project: AeProject,
    indices: BTreeMap<ItemId, usize>,
    topological_ids: Vec<ItemId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionClosure {
    pub root_id: ItemId,
    /// Dependency-first order, including footage/unsupported dependencies.
    pub item_ids: Vec<ItemId>,
    pub composition_ids: Vec<ItemId>,
    /// Explicit unsupported data and intrinsically unconvertible source kinds.
    /// An empty list is NOT a native renderer compatibility certificate.
    pub diagnostics: Vec<Diagnostic>,
}
impl CompositionClosure {
    pub fn is_supported(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

impl ValidatedProject {
    pub fn project(&self) -> &AeProject {
        &self.project
    }
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.indices
            .get(&id)
            .map(|index| &self.project.items[*index])
    }
    pub fn composition(&self, id: ItemId) -> Option<&Composition> {
        match self.item(id) {
            Some(Item::Composition(value)) => Some(value),
            _ => None,
        }
    }
    pub fn composition_closure(&self, root_id: ItemId) -> Result<CompositionClosure, ReadError> {
        if self.composition(root_id).is_none() {
            return fail(
                DiagnosticCode::DanglingReference,
                "root_id",
                "Root is not an existing composition",
            );
        }
        let mut reachable = BTreeSet::new();
        let mut queue = vec![root_id];
        while let Some(id) = queue.pop() {
            if reachable.insert(id) {
                queue.extend(dependencies(self.item(id).expect("validated item link")));
            }
        }
        let item_ids: Vec<_> = self
            .topological_ids
            .iter()
            .copied()
            .filter(|id| reachable.contains(id))
            .collect();
        let mut result = CompositionClosure {
            root_id,
            item_ids,
            composition_ids: vec![],
            diagnostics: vec![],
        };
        add_unsupported(
            &mut result.diagnostics,
            "unsupported",
            &self.project.unsupported,
        );
        for id in &result.item_ids {
            let path = format!("items[{id}]");
            match self.item(*id).expect("validated item link") {
                Item::Composition(comp) => {
                    result.composition_ids.push(*id);
                    add_unsupported(&mut result.diagnostics, &path, &comp.unsupported);
                    for layer in &comp.layers {
                        let path = format!("{path}.layers[{}]", layer.id);
                        add_unsupported(&mut result.diagnostics, &path, &layer.unsupported);
                        if layer.three_d {
                            result.diagnostics.push(Diagnostic::new(
                                DiagnosticCode::UnsupportedFeature,
                                &path,
                                "Three-dimensional layer semantics are not supported by this slice",
                            ));
                        }
                        for (i, prop) in layer.properties.iter().enumerate() {
                            add_unsupported(
                                &mut result.diagnostics,
                                &format!("{path}.properties[{i}]"),
                                &prop.unsupported,
                            );
                        }
                        for slider in &layer.sliders {
                            add_unsupported(
                                &mut result.diagnostics,
                                &format!("{path}.sliders[{}]", slider.id),
                                &slider.property.unsupported,
                            );
                        }
                        match &layer.source {
                            LayerSource::Unsupported { feature, .. } => add_unsupported(
                                &mut result.diagnostics,
                                &path,
                                std::slice::from_ref(feature),
                            ),
                            LayerSource::Text { document, keys, .. } => {
                                add_unsupported(
                                    &mut result.diagnostics,
                                    &format!("{path}.document"),
                                    &document.unsupported,
                                );
                                for (i, key) in keys.iter().enumerate() {
                                    add_unsupported(
                                        &mut result.diagnostics,
                                        &format!("{path}.text_keys[{i}]"),
                                        &key.document.unsupported,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Item::Footage(footage) => {
                    add_unsupported(&mut result.diagnostics, &path, &footage.unsupported);
                    result.diagnostics.push(Diagnostic::new(DiagnosticCode::UnsupportedFeature, &path, "Footage metadata has no verified decoded asset; external media conversion is not implemented"));
                }
                Item::Unsupported(item) => {
                    add_unsupported(&mut result.diagnostics, &path, &item.unsupported)
                }
            }
        }
        Ok(result)
    }
}

fn add_unsupported(out: &mut Vec<Diagnostic>, path: &str, features: &[UnsupportedFeature]) {
    for feature in features {
        out.push(Diagnostic::new(
            DiagnosticCode::UnsupportedFeature,
            path,
            format!("{}: {}", feature.code, feature.detail),
        ));
    }
}

pub fn parse_json(bytes: &[u8], limits: &Limits) -> Result<ValidatedProject, ReadError> {
    if bytes.len() > limits.max_file_bytes {
        return fail(
            DiagnosticCode::ResourceLimit,
            "$",
            "Interchange exceeds file byte limit",
        );
    }
    // serde_json's recursion guard remains enabled; never parse a Value first,
    // since that would discard duplicate object members before strict decoding.
    let project = serde_json::from_slice(bytes).map_err(|error| {
        ReadError::from(Diagnostic::new(
            DiagnosticCode::InvalidJson,
            "$",
            format!("Strict schema decoding failed: {error}"),
        ))
    })?;
    validate_project(project, limits)
}

pub fn validate_project(
    project: AeProject,
    limits: &Limits,
) -> Result<ValidatedProject, ReadError> {
    if project.schema_version != 1 {
        return fail(
            DiagnosticCode::UnsupportedSchema,
            "schema_version",
            "Only interchange schema 1 is supported",
        );
    }
    let mut check = Validator::new(limits);
    check.count(project.items.len(), limits.max_items, "items", "Items")?;
    if project.items.is_empty() {
        return invalid("items", "At least one item is required");
    }
    check.string(
        &project.provenance.source_name,
        "provenance.source_name",
        true,
    )?;
    check.string(&project.provenance.producer, "provenance.producer", true)?;
    check.string(
        &project.provenance.producer_version,
        "provenance.producer_version",
        true,
    )?;
    if let Some(digest) = &project.provenance.source_sha256 {
        check.string(digest, "provenance.source_sha256", true)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return invalid("provenance.source_sha256", "Expected lowercase SHA-256 hex");
        }
    }
    check.unsupported(&project.unsupported, "unsupported")?;
    let mut indices = BTreeMap::new();
    let mut layer_ids = BTreeSet::new();
    for (index, item) in project.items.iter().enumerate() {
        let path = format!("items[{index}]");
        id(item.id(), &path)?;
        if indices.insert(item.id(), index).is_some() {
            return fail(DiagnosticCode::DuplicateId, &path, "Duplicate item ID");
        }
        check.string(item.name(), &format!("{path}.name"), true)?;
        match item {
            Item::Composition(comp) => {
                check.dimensions(comp.width, comp.height, &path)?;
                check.ratio(comp.pixel_aspect, 100, &format!("{path}.pixel_aspect"))?;
                check.ratio(comp.frame_rate, 1000, &format!("{path}.frame_rate"))?;
                check.time(comp.duration, &format!("{path}.duration"))?;
                if comp.duration.numerator <= 0 {
                    return invalid(&path, "Composition duration must be positive");
                }
                check.unsupported(&comp.unsupported, &path)?;
                check.markers(&comp.markers, &format!("{path}.markers"))?;
                check.layers = add_count(
                    check.layers,
                    comp.layers.len(),
                    limits.max_layers,
                    &path,
                    "Layers",
                )?;
                let local_ids: BTreeSet<_> = comp.layers.iter().map(|layer| layer.id).collect();
                let parents: BTreeMap<_, _> = comp
                    .layers
                    .iter()
                    .map(|layer| (layer.id, layer.parent_id))
                    .collect();
                for (i, layer) in comp.layers.iter().enumerate() {
                    let path = format!("{path}.layers[{i}]");
                    id(layer.id, &path)?;
                    if !layer_ids.insert(layer.id) {
                        return fail(
                            DiagnosticCode::DuplicateId,
                            &path,
                            "Duplicate global layer ID",
                        );
                    }
                    if layer
                        .parent_id
                        .is_some_and(|parent| !local_ids.contains(&parent))
                    {
                        return fail(
                            DiagnosticCode::DanglingReference,
                            &path,
                            "Parent must be a layer in the same composition",
                        );
                    }
                    check.layer(layer, &path)?;
                }
                validate_parent_graph(&parents, limits.max_dependency_depth, &path)?;
            }
            Item::Footage(footage) => {
                check.string(&footage.source_path, &format!("{path}.source_path"), true)?;
                check.dimensions(footage.width, footage.height, &path)?;
                check.ratio(footage.pixel_aspect, 100, &path)?;
                if let Some(rate) = footage.frame_rate {
                    check.ratio(rate, 1000, &path)?;
                }
                if let Some(duration) = footage.duration {
                    check.time(duration, &path)?;
                    if duration.numerator < 0 {
                        return invalid(&path, "Footage duration cannot be negative");
                    }
                }
                check.unsupported(&footage.unsupported, &path)?;
            }
            Item::Unsupported(item) => {
                if item.unsupported.is_empty() {
                    return invalid(&path, "Unsupported item requires a diagnostic");
                }
                check.unsupported(&item.unsupported, &path)?;
                check.count(
                    item.dependency_ids.len(),
                    limits.max_items,
                    &path,
                    "Dependencies",
                )?;
            }
        }
    }
    if project.root_composition_ids.is_empty() {
        return invalid(
            "root_composition_ids",
            "At least one root composition is required",
        );
    }
    check.count(
        project.root_composition_ids.len(),
        limits.max_items,
        "root_composition_ids",
        "Roots",
    )?;
    let mut roots = BTreeSet::new();
    for root in &project.root_composition_ids {
        if !roots.insert(*root) {
            return fail(
                DiagnosticCode::DuplicateId,
                "root_composition_ids",
                "Duplicate root ID",
            );
        }
        if !matches!(
            indices.get(root).map(|i| &project.items[*i]),
            Some(Item::Composition(_))
        ) {
            return fail(
                DiagnosticCode::DanglingReference,
                "root_composition_ids",
                "Root must reference an existing composition",
            );
        }
    }
    let mut links = 0;
    for item in &project.items {
        let path = format!("items[{}]", item.id());
        let deps = dependencies(item);
        links = add_count(
            links,
            deps.len(),
            limits.max_links,
            &path,
            "Item dependency links",
        )?;
        for dep in deps {
            if !indices.contains_key(&dep) {
                return fail(
                    DiagnosticCode::DanglingReference,
                    &path,
                    format!("Missing dependency item ID {dep}"),
                );
            }
        }
        if let Item::Composition(comp) = item {
            for layer in &comp.layers {
                let valid = match layer.source {
                    LayerSource::Composition { item_id } => {
                        matches!(&project.items[indices[&item_id]], Item::Composition(_))
                    }
                    LayerSource::Footage { item_id } => {
                        matches!(&project.items[indices[&item_id]], Item::Footage(_))
                    }
                    _ => true,
                };
                if !valid {
                    return invalid(&path, "Layer source item has the wrong type");
                }
            }
        }
    }
    let topological_ids =
        validate_item_graph(&project.items, &indices, limits.max_dependency_depth)?;
    Ok(ValidatedProject {
        project,
        indices,
        topological_ids,
    })
}

fn dependencies(item: &Item) -> Vec<ItemId> {
    match item {
        Item::Composition(comp) => comp
            .layers
            .iter()
            .flat_map(|layer| match &layer.source {
                LayerSource::Composition { item_id } | LayerSource::Footage { item_id } => {
                    vec![*item_id]
                }
                LayerSource::Unsupported { dependency_ids, .. } => dependency_ids.clone(),
                _ => vec![],
            })
            .collect(),
        Item::Unsupported(item) => item.dependency_ids.clone(),
        Item::Footage(_) => vec![],
    }
}

fn validate_parent_graph(
    parents: &BTreeMap<LayerId, Option<LayerId>>,
    max_depth: usize,
    path: &str,
) -> Result<(), ReadError> {
    for root in parents.keys() {
        let mut visited = BTreeSet::new();
        let mut current = Some(*root);
        while let Some(id) = current {
            if !visited.insert(id) {
                return fail(
                    DiagnosticCode::CyclicReference,
                    path,
                    "Layer parenting cycle",
                );
            }
            if visited.len() > max_depth {
                return fail(
                    DiagnosticCode::ResourceLimit,
                    path,
                    "Layer parenting depth exceeded",
                );
            }
            current = parents[&id];
        }
    }
    Ok(())
}

fn validate_item_graph(
    items: &[Item],
    indices: &BTreeMap<ItemId, usize>,
    max_depth: usize,
) -> Result<Vec<ItemId>, ReadError> {
    let edges: BTreeMap<_, _> = items
        .iter()
        .map(|item| (item.id(), dependencies(item)))
        .collect();
    let mut state = BTreeMap::<ItemId, u8>::new();
    let mut heights = BTreeMap::<ItemId, usize>::new();
    let mut result = vec![];
    // Iterative DFS plus longest-path accounting. Checking only stack depth misses
    // a deep shared dependency that was completed through an earlier shallow root.
    for id in indices.keys() {
        if state.get(id) == Some(&2) {
            continue;
        }
        let mut stack = vec![(*id, 0usize)];
        state.insert(*id, 1);
        while !stack.is_empty() {
            if stack.len() > max_depth {
                return fail(
                    DiagnosticCode::ResourceLimit,
                    "items",
                    "Item dependency depth exceeded",
                );
            }
            let (node, cursor) = stack.last_mut().expect("nonempty stack");
            let node = *node;
            if *cursor < edges[&node].len() {
                let next = edges[&node][*cursor];
                *cursor += 1;
                match state.get(&next) {
                    Some(1) => {
                        return fail(
                            DiagnosticCode::CyclicReference,
                            "items",
                            "Item dependency cycle",
                        );
                    }
                    Some(2) => {}
                    _ => {
                        state.insert(next, 1);
                        stack.push((next, 0));
                    }
                }
            } else {
                let height = edges[&node]
                    .iter()
                    .map(|dep| heights[dep])
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| {
                        ReadError::from(Diagnostic::new(
                            DiagnosticCode::ResourceLimit,
                            "items",
                            "Dependency height overflow",
                        ))
                    })?;
                if height > max_depth {
                    return fail(
                        DiagnosticCode::ResourceLimit,
                        "items",
                        "Item dependency depth exceeded",
                    );
                }
                heights.insert(node, height);
                state.insert(node, 2);
                result.push(node);
                stack.pop();
            }
        }
    }
    Ok(result)
}

struct Validator<'a> {
    limits: &'a Limits,
    strings: usize,
    programs: usize,
    program_bytes: usize,
    layers: usize,
    properties: usize,
    keys: usize,
    marker_count: usize,
    runs: usize,
    unsupported_count: usize,
}
impl<'a> Validator<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            strings: 0,
            programs: 0,
            program_bytes: 0,
            layers: 0,
            properties: 0,
            keys: 0,
            marker_count: 0,
            runs: 0,
            unsupported_count: 0,
        }
    }
    fn count(&self, count: usize, max: usize, path: &str, name: &str) -> Result<(), ReadError> {
        add_count(0, count, max, path, name).map(|_| ())
    }
    fn string(&mut self, value: &str, path: &str, nonempty: bool) -> Result<(), ReadError> {
        if nonempty && value.trim().is_empty() {
            return invalid(path, "Nonempty string required");
        }
        if value.contains('\0') {
            return invalid(path, "NUL is not allowed in interchange strings");
        }
        self.count(
            value.len(),
            self.limits.max_string_bytes,
            path,
            "String bytes",
        )?;
        self.strings = add_count(
            self.strings,
            value.len(),
            self.limits.max_total_string_bytes,
            path,
            "Total string bytes",
        )?;
        Ok(())
    }
    fn unsupported(&mut self, values: &[UnsupportedFeature], path: &str) -> Result<(), ReadError> {
        self.unsupported_count = add_count(
            self.unsupported_count,
            values.len(),
            self.limits.max_unsupported,
            path,
            "Unsupported features",
        )?;
        for value in values {
            self.string(&value.code, path, true)?;
            self.string(&value.detail, path, true)?;
        }
        Ok(())
    }
    fn dimensions(&self, width: u32, height: u32, path: &str) -> Result<(), ReadError> {
        if width == 0
            || height == 0
            || width > self.limits.max_dimension
            || height > self.limits.max_dimension
        {
            return invalid(path, "Dimensions are zero or exceed limit");
        }
        Ok(())
    }
    fn ratio(&self, ratio: FrameRate, max: u32, path: &str) -> Result<(), ReadError> {
        if ratio.numerator == 0
            || ratio.denominator == 0
            || u64::from(ratio.numerator) > u64::from(max) * u64::from(ratio.denominator)
        {
            return invalid(path, "Invalid positive rational rate/aspect");
        }
        Ok(())
    }
    fn time(&self, time: RationalTime, path: &str) -> Result<(), ReadError> {
        if time.denominator == 0
            || i128::from(time.numerator).abs()
                > i128::from(self.limits.max_abs_time_seconds) * i128::from(time.denominator)
        {
            return invalid(path, "Time has zero denominator or exceeds range");
        }
        Ok(())
    }
    fn layer(&mut self, layer: &Layer, path: &str) -> Result<(), ReadError> {
        self.string(&layer.name, path, true)?;
        self.time(layer.start_time, &format!("{path}.start_time"))?;
        self.time(layer.in_point, &format!("{path}.in_point"))?;
        self.time(layer.out_point, &format!("{path}.out_point"))?;
        if layer.in_point.compare(layer.out_point).is_ge() {
            return invalid(path, "Layer in point must precede out point");
        }
        label(layer.label, path)?;
        self.unsupported(&layer.unsupported, path)?;
        self.markers(&layer.markers, &format!("{path}.markers"))?;
        let mut paths = BTreeSet::new();
        for (i, property) in layer.properties.iter().enumerate() {
            if !paths.insert(&property.match_names) {
                return fail(
                    DiagnosticCode::DuplicateId,
                    path,
                    "Duplicate property match-name path",
                );
            }
            self.property(property, &format!("{path}.properties[{i}]"))?;
        }
        let mut slider_ids = BTreeSet::new();
        for (i, slider) in layer.sliders.iter().enumerate() {
            let path = format!("{path}.sliders[{i}]");
            id(slider.id, &path)?;
            if !slider_ids.insert(slider.id) {
                return fail(
                    DiagnosticCode::DuplicateId,
                    &path,
                    "Duplicate slider effect ID",
                );
            }
            self.string(&slider.name, &path, true)?;
            if slider.property.dimensions != 1
                || slider.property.match_names
                    != ["ADBE Slider Control", "ADBE Slider Control-0001"]
            {
                return invalid(
                    &path,
                    "Named Slider requires one dimension and exact Slider Control match-name path",
                );
            }
            self.property(&slider.property, &path)?;
        }
        match &layer.source {
            LayerSource::Null { width, height } => self.dimensions(*width, *height, path)?,
            LayerSource::Solid {
                width,
                height,
                color,
            } => {
                self.dimensions(*width, *height, path)?;
                rgb(color, path)?;
            }
            LayerSource::Text {
                width,
                height,
                document,
                keys,
            } => {
                self.dimensions(*width, *height, path)?;
                self.text(document, &format!("{path}.document"))?;
                self.keys = add_count(self.keys, keys.len(), self.limits.max_keys, path, "Keys")?;
                let mut previous = None;
                for (i, key) in keys.iter().enumerate() {
                    let path = format!("{path}.text_keys[{i}]");
                    self.ordered_time(key.time, &mut previous, &path)?;
                    self.text(&key.document, &path)?;
                }
            }
            LayerSource::Unsupported {
                dependency_ids,
                feature,
            } => {
                self.count(
                    dependency_ids.len(),
                    self.limits.max_items,
                    path,
                    "Dependencies",
                )?;
                self.unsupported(std::slice::from_ref(feature), path)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn ordered_time(
        &self,
        time: RationalTime,
        previous: &mut Option<RationalTime>,
        path: &str,
    ) -> Result<(), ReadError> {
        self.time(time, path)?;
        if previous.is_some_and(|value| value.compare(time).is_ge()) {
            return invalid(
                path,
                "Times must be strictly increasing without rationally equivalent duplicates",
            );
        }
        *previous = Some(time);
        Ok(())
    }
    fn property(&mut self, property: &NumericProperty, path: &str) -> Result<(), ReadError> {
        self.properties = add_count(
            self.properties,
            1,
            self.limits.max_properties,
            path,
            "Properties",
        )?;
        if !(1..=4).contains(&property.dimensions)
            || property.match_names.is_empty()
            || property.match_names.len() > 16
        {
            return invalid(
                path,
                "Property requires 1..4 dimensions and 1..16 match-name path components",
            );
        }
        for name in &property.match_names {
            self.string(name, path, true)?;
        }
        if let Some(value) = &property.value {
            vector(value, property.dimensions, path)?;
        }
        self.unsupported(&property.unsupported, path)?;
        self.keys = add_count(
            self.keys,
            property.keys.len(),
            self.limits.max_keys,
            path,
            "Keys",
        )?;
        let mut previous = None;
        for (i, key) in property.keys.iter().enumerate() {
            let path = format!("{path}.keys[{i}]");
            self.ordered_time(key.time, &mut previous, &path)?;
            vector(&key.value, property.dimensions, &path)?;
            for ease in [&key.in_ease, &key.out_ease].into_iter().flatten() {
                if ease.len() != usize::from(property.dimensions) {
                    return invalid(&path, "Ease dimension mismatch");
                }
                for component in ease {
                    finite(component.speed, &path)?;
                    range(component.influence, 0.1, 100.0, &path)?;
                }
            }
        }
        if let Some(expression) = &property.expression {
            self.programs =
                add_count(self.programs, 1, self.limits.max_programs, path, "Programs")?;
            self.count(
                expression.source.len(),
                self.limits.max_program_bytes,
                path,
                "Program bytes",
            )?;
            self.program_bytes = add_count(
                self.program_bytes,
                expression.source.len(),
                self.limits.max_total_program_bytes,
                path,
                "Total program bytes",
            )?;
            self.string(&expression.source, path, expression.enabled)?;
        }
        Ok(())
    }
    fn markers(&mut self, markers: &[Marker], path: &str) -> Result<(), ReadError> {
        self.marker_count = add_count(
            self.marker_count,
            markers.len(),
            self.limits.max_markers,
            path,
            "Markers",
        )?;
        let mut previous = None;
        for (i, marker) in markers.iter().enumerate() {
            let path = format!("{path}[{i}]");
            self.ordered_time(marker.time, &mut previous, &path)?;
            self.time(marker.duration, &path)?;
            if marker.duration.numerator < 0 {
                return invalid(&path, "Marker duration cannot be negative");
            }
            label(marker.label, &path)?;
            for text in [
                &marker.comment,
                &marker.chapter,
                &marker.url,
                &marker.frame_target,
                &marker.cue_point_name,
            ] {
                self.string(text, &path, false)?;
            }
            self.count(marker.parameters.len(), 1024, &path, "Marker parameters")?;
            let mut parameters = BTreeSet::new();
            for parameter in &marker.parameters {
                if !parameters.insert(&parameter.name) {
                    return fail(
                        DiagnosticCode::DuplicateId,
                        &path,
                        "Duplicate marker parameter name",
                    );
                }
                self.string(&parameter.name, &path, true)?;
                self.string(&parameter.value, &path, false)?;
            }
        }
        Ok(())
    }
    fn text(&mut self, document: &TextDocument, path: &str) -> Result<(), ReadError> {
        self.string(&document.text, path, false)?;
        self.character_style(&document.default_style, path)?;
        self.unsupported(&document.unsupported, path)?;
        self.runs = add_count(
            self.runs,
            document.runs.len(),
            self.limits.max_runs,
            path,
            "Text runs",
        )?;
        if !document.runs.is_empty() {
            let mut boundaries = BTreeSet::from([0usize]);
            let mut length = 0;
            for ch in document.text.chars() {
                length += ch.len_utf16();
                boundaries.insert(length);
            }
            let mut previous = 0;
            for run in &document.runs {
                let start = run.start_utf16 as usize;
                let end = run.end_utf16 as usize;
                if start != previous
                    || end <= start
                    || !boundaries.contains(&start)
                    || !boundaries.contains(&end)
                {
                    return invalid(
                        path,
                        "Text runs must completely cover text on UTF-16 scalar boundaries without gaps/overlaps",
                    );
                }
                self.character_style(&run.style, path)?;
                previous = end;
            }
            if previous != length {
                return invalid(path, "Text run coverage is incomplete");
            }
        }
        let paragraph = &document.paragraph;
        range(paragraph.leading, 0.1, 10.0, path)?;
        if let Some(size) = paragraph.box_size {
            for dimension in size {
                range(
                    dimension,
                    f64::MIN_POSITIVE,
                    f64::from(self.limits.max_dimension),
                    path,
                )?;
            }
        }
        for number in [
            paragraph.left_indent,
            paragraph.right_indent,
            paragraph.first_line_indent,
            paragraph.space_before,
            paragraph.space_after,
        ] {
            range(number, -1_000_000.0, 1_000_000.0, path)?;
        }
        Ok(())
    }
    fn character_style(&mut self, style: &CharacterStyle, path: &str) -> Result<(), ReadError> {
        self.string(&style.font.postscript_name, path, true)?;
        self.string(&style.font.family, path, true)?;
        self.string(&style.font.style, path, true)?;
        if !(1..=1000).contains(&style.font.weight) {
            return invalid(path, "Font weight must be 1..1000");
        }
        range(style.font_size, 0.1, 10_000.0, path)?;
        if let Some(color) = &style.fill_rgb {
            rgb(color, path)?;
        }
        if let Some(color) = &style.stroke_rgb {
            rgb(color, path)?;
        }
        range(style.stroke_width, 0.0, 10_000.0, path)?;
        range(style.tracking, -10_000.0, 100_000.0, path)?;
        range(style.baseline_shift, -1_000_000.0, 1_000_000.0, path)?;
        range(style.horizontal_scale, 0.1, 10_000.0, path)?;
        range(style.vertical_scale, 0.1, 10_000.0, path)
    }
}
fn id(value: u64, path: &str) -> Result<(), ReadError> {
    if value == 0 {
        invalid(path, "IDs must be nonzero")
    } else {
        Ok(())
    }
}
fn label(value: u8, path: &str) -> Result<(), ReadError> {
    if value > 16 {
        invalid(path, "AE label index must be 0..16")
    } else {
        Ok(())
    }
}
fn finite(value: f64, path: &str) -> Result<(), ReadError> {
    if value.is_finite() {
        Ok(())
    } else {
        invalid(path, "Nonfinite numeric value")
    }
}
fn range(value: f64, min: f64, max: f64, path: &str) -> Result<(), ReadError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        invalid(path, "Numeric value outside supported interchange range")
    }
}
fn rgb(value: &[f64; 3], path: &str) -> Result<(), ReadError> {
    for component in value {
        range(*component, 0.0, 1.0, path)?;
    }
    Ok(())
}
fn vector(value: &[f64], dimensions: u8, path: &str) -> Result<(), ReadError> {
    if value.len() != usize::from(dimensions) {
        return invalid(path, "Numeric vector dimension mismatch");
    }
    for component in value {
        finite(*component, path)?;
    }
    Ok(())
}
fn add_count(
    current: usize,
    increment: usize,
    max: usize,
    path: &str,
    name: &str,
) -> Result<usize, ReadError> {
    match current.checked_add(increment) {
        Some(value) if value <= max => Ok(value),
        _ => fail(
            DiagnosticCode::ResourceLimit,
            path,
            format!("{name} limit exceeded"),
        ),
    }
}
fn invalid<T>(path: &str, message: &str) -> Result<T, ReadError> {
    fail(DiagnosticCode::InvalidValue, path, message)
}
