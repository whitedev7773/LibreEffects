//! Explicit fixed-axis perspective for front-parallel native layer planes.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Camera3 {
    pub position: [f64; 3],
    pub focal_distance: f64,
    pub principal_point: [f64; 2],
    pub near_clip: f64,
}
impl Camera3 {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .position
            .iter()
            .chain(&self.principal_point)
            .any(|v| !v.is_finite() || v.abs() > 1e9)
            || !self.focal_distance.is_finite()
            || !(0.0..=1e9).contains(&self.focal_distance)
            || self.focal_distance == 0.
            || !self.near_clip.is_finite()
            || !(0.0..=1e9).contains(&self.near_clip)
            || self.near_clip == 0.
        {
            return Err("Camera coordinates must be finite and bounded, with positive focal distance and near clip".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedLayerGeometry {
    pub transform: Affine,
    pub inverse: Option<Affine>,
    pub corners: [[f64; 2]; 4],
    pub depth: Option<f64>,
}
pub(super) fn validate_comp(comp: &Composition, version: u32) -> Result<(), String> {
    if let Some(camera) = &comp.camera {
        if version < 72 {
            return Err("Explicit spatial camera requires project version 72".into());
        }
        camera.validate()?;
    }
    if comp.has_spatial_layers()
        && comp
            .layers
            .iter()
            .any(|layer| layer.expressions.iter().any(|program| program.enabled))
    {
        return Err("Expression evaluation in spatial compositions is not yet supported".into());
    }
    for layer in &comp.layers {
        if let Some(parent) = layer.parent.and_then(|id| comp.layer(id)) {
            if parent.is_three_d() != layer.is_three_d() {
                return Err("Mixed-dimensional parenting is unsupported".into());
            }
        }
    }
    Ok(())
}
impl Composition {
    fn plane_transform(&self, id: LayerId, frame: Frame) -> Result<(Affine, f64), String> {
        let mut current = Some(id);
        let mut transform = Affine::default();
        let mut z = 0.;
        let mut visited = BTreeSet::new();
        while let Some(id) = current {
            if !visited.insert(id) || visited.len() > self.layers.len() {
                return Err("Spatial parent cycle".into());
            }
            let layer = self.layer(id).ok_or("Spatial parent is missing")?;
            if !layer.is_three_d() {
                return Err("Mixed-dimensional parenting is unsupported".into());
            }
            if layer.transform_offset != Affine::default() {
                return Err("Spatial transform compensation is unsupported".into());
            }
            let [x, y, local_z] = layer.position3_at(frame, self.fps.seconds(1))?;
            let value = |p| {
                layer
                    .property(p)
                    .map(|t| t.value_at(frame))
                    .ok_or("Missing scalar plane transform")
            };
            let (sin, cos) = value(Property::Rotation)?.to_radians().sin_cos();
            let sx = value(Property::ScaleX)? / 100.;
            let sy = value(Property::ScaleY)? / 100.;
            let [a, b, c, d] = [cos * sx, sin * sx, -sin * sy, cos * sy];
            let local = Affine([
                a,
                b,
                c,
                d,
                x - a * value(Property::AnchorX)? - c * value(Property::AnchorY)?,
                y - b * value(Property::AnchorX)? - d * value(Property::AnchorY)?,
            ]);
            transform = local.compose(transform);
            z += local_z;
            if !transform.valid() || !z.is_finite() {
                return Err("Spatial hierarchy exceeds finite geometry bounds".into());
            }
            current = layer.parent;
        }
        Ok((transform, z))
    }
    /// One authoritative local-source-pixel to composition-pixel result. Text
    /// consumers transform their shaped bounds with this matrix, not nominal size.
    pub fn projected_geometry(
        &self,
        id: LayerId,
        frame: Frame,
    ) -> Result<ProjectedLayerGeometry, String> {
        let layer = self.layer(id).ok_or("Layer not found")?;
        if let Some(matte) = layer.track_matte {
            let source = self.layer(matte.source).ok_or("Matte source not found")?;
            if source.is_three_d() != layer.is_three_d() {
                return Err("Mixed-dimensional mattes are unsupported".into());
            }
        }
        let (transform, depth) = if layer.is_three_d() {
            let camera = self
                .camera
                .as_ref()
                .ok_or("Spatial rendering requires an explicit native camera")?;
            camera.validate()?;
            let (world, z) = self.plane_transform(id, frame)?;
            let depth = z - camera.position[2];
            if !depth.is_finite() || depth <= camera.near_clip {
                return Err("Spatial plane is on or behind the camera near plane".into());
            }
            let scale = camera.focal_distance / depth;
            let camera_transform = Affine([
                scale,
                0.,
                0.,
                scale,
                camera.principal_point[0] - scale * camera.position[0],
                camera.principal_point[1] - scale * camera.position[1],
            ]);
            (camera_transform.compose(world), Some(depth))
        } else {
            (
                self.world_transform(id, frame)
                    .ok_or("Unsupported or invalid 2D layer transform")?,
                None,
            )
        };
        if !transform.valid() {
            return Err("Projected geometry exceeds finite bounds".into());
        }
        let corners = [
            [0., 0.],
            [layer.width, 0.],
            [layer.width, layer.height],
            [0., layer.height],
        ]
        .map(|point| transform.point(point));
        if corners
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e12)
        {
            return Err("Projected corners exceed finite bounds".into());
        }
        Ok(ProjectedLayerGeometry {
            transform,
            inverse: transform.inverse(),
            corners,
            depth,
        })
    }
    /// Paint order. Equal-depth planes preserve ordinary back-to-front stack
    /// order. Non-painting Null/Audio sources are excluded; preview controls
    /// handle their overlays separately from pixel admission.
    pub fn render_order(&self, frame: Frame, include_guides: bool) -> Result<Vec<LayerId>, String> {
        let layers = self
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| {
                self.layer_active(layer, frame, include_guides)
                    && !matches!(layer.content, Content::Audio { .. } | Content::Null)
            })
            .collect::<Vec<_>>();
        let has_spatial = layers.iter().any(|(_, layer)| layer.is_three_d());
        if has_spatial && layers.iter().any(|(_, layer)| !layer.is_three_d()) {
            return Err("Mixed visible 2D and spatial layer planes are unsupported".into());
        }
        if !has_spatial {
            return Ok(layers
                .into_iter()
                .rev()
                .map(|(_, layer)| layer.id)
                .collect());
        }
        let mut ordered = layers
            .into_iter()
            .map(|(index, layer)| {
                self.projected_geometry(layer.id, frame)
                    .map(|g| (index, layer.id, g.depth.unwrap()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        ordered.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| b.0.cmp(&a.0)));
        Ok(ordered.into_iter().map(|(_, id, _)| id).collect())
    }
}
