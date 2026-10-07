//! Strict conversion of the independently authored AE interchange contract.
//! Conversion creates a detached project. It never executes source expressions,
//! opens media paths, guesses binary payloads or partially edits an open document.
mod convert;
use libre_effects_ae_project as ae;
use libre_effects_core::Project;

pub struct ImportDocument {
    source: ae::ValidatedProject,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootSummary {
    pub id: u64,
    pub name: String,
    pub layers: usize,
    pub blocker: Option<String>,
}
impl ImportDocument {
    pub fn roots(&self) -> Vec<RootSummary> {
        self.roots_with_preflight(|_| Ok(()))
    }
    pub fn roots_with_preflight(
        &self,
        preflight: impl Fn(&Project) -> Result<(), String>,
    ) -> Vec<RootSummary> {
        self.source
            .project()
            .root_composition_ids
            .iter()
            .filter_map(|id| {
                let comp = self.source.composition(*id)?;
                Some(RootSummary {
                    id: *id,
                    name: comp.name.clone(),
                    layers: comp.layers.len(),
                    blocker: self
                        .convert(*id)
                        .and_then(|project| preflight(&project))
                        .err(),
                })
            })
            .collect()
    }
    pub fn convert(&self, root: u64) -> Result<Project, String> {
        convert::convert(&self.source, root)
    }
    pub fn source(&self) -> &ae::AeProject {
        self.source.project()
    }
}
pub fn read_document(bytes: &[u8]) -> Result<ImportDocument, String> {
    if bytes.starts_with(b"RIFX") {
        let inventory =
            ae::inspect_rifx(bytes, &ae::Limits::default()).map_err(|e| e.to_string())?;
        let _ = inventory;
        return Err("RIFX container is structurally readable, but its binary payload schema is not verified. Use an explicit supported typed interchange document; no project was imported.".into());
    }
    let limits = ae::Limits {
        max_items: 100,
        max_layers: 1_000,
        max_properties: 10_000,
        max_keys: 50_000,
        ..Default::default()
    };
    ae::parse_json(bytes, &limits)
        .map(|source| ImportDocument { source })
        .map_err(|e| e.to_string())
}
