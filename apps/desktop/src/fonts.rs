//! Shared font discovery and matching for the Character panel and every renderer.
use libre_effects_core::TextStyle;
use resvg::usvg::fontdb::{Database, Family, Query, Stretch, Style, Weight};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicUsize, Ordering},
};

const AUTHORED_FONT_FILE_LIMIT: usize = 64 * 1024 * 1024;
const AUTHORED_FONT_CACHE_LIMIT: usize = 256 * 1024 * 1024;

pub(crate) fn render_options() -> resvg::usvg::Options<'static> {
    resvg::usvg::Options {
        fontdb: database(),
        font_family: "Wanted Sans".into(),
        ..Default::default()
    }
}

struct Catalog {
    db: Arc<Database>,
    families: Vec<String>,
    aliases: std::collections::HashMap<resvg::usvg::fontdb::ID, String>,
    authored:
        std::collections::HashMap<resvg::usvg::fontdb::ID, OnceLock<Result<AuthoredFont, String>>>,
    authored_bytes: AtomicUsize,
}
fn catalog() -> &'static Catalog {
    static FONTS: OnceLock<Catalog> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = Database::new();
        db.load_system_fonts();
        // Always use the shipped Wanted Sans files, even if another version is installed.
        let duplicates: Vec<_> = db
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n == "Wanted Sans"))
            .map(|f| f.id)
            .collect();
        for id in duplicates {
            db.remove_face(id);
        }
        for data in [
            include_bytes!("../assets/fonts/WantedSans-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/WantedSans-Medium.ttf").as_slice(),
            include_bytes!("../assets/fonts/WantedSans-SemiBold.ttf").as_slice(),
            include_bytes!("../assets/fonts/WantedSans-Bold.ttf").as_slice(),
            include_bytes!("../assets/fonts/WantedSans-ExtraBold.ttf").as_slice(),
            include_bytes!("../assets/fonts/WantedSans-Black.ttf").as_slice(),
            include_bytes!("../assets/fonts/WantedSans-ExtraBlack.ttf").as_slice(),
        ] {
            db.load_font_data(data.to_vec());
        }
        let families: std::collections::BTreeSet<_> = db
            .faces()
            .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
            .collect();
        let mut families: Vec<_> = families.into_iter().collect();
        families.sort_by_cached_key(|s| s.to_lowercase());
        let faces: Vec<_> = db.faces().cloned().collect();
        let mut aliases = std::collections::HashMap::new();
        let mut authored = std::collections::HashMap::new();
        for (i, mut face) in faces.into_iter().enumerate() {
            db.remove_face(face.id);
            let alias = format!("LibreEffectsFont{i}");
            let language = face.families[0].1;
            face.families.push((alias.clone(), language));
            let id = db.push_face_info(face);
            aliases.insert(id, alias);
            authored.insert(id, OnceLock::new());
        }
        Catalog {
            db: Arc::new(db),
            families,
            aliases,
            authored,
            authored_bytes: AtomicUsize::new(0),
        }
    })
}
pub(crate) fn database() -> Arc<Database> {
    catalog().db.clone()
}
/// An immutable, lazily loaded single-face database for authored positioning.
/// Ordinary documents never retain extra system-font bytes. Positioned paint
/// is flattened from this database, so final rendering cannot reopen a file
/// that changed after identity verification.
pub(crate) struct AuthoredFont {
    pub id: resvg::usvg::fontdb::ID,
    pub sha256: String,
    pub index: u32,
    pub post_script_name: String,
    db: Arc<Database>,
    _reservation: FontReservation,
}
struct FontReservation(usize);
impl FontReservation {
    fn reserve(bytes: usize) -> Result<Self, String> {
        if bytes > AUTHORED_FONT_FILE_LIMIT {
            return Err("The required font exceeds the 64 MiB authored-font file limit".into());
        }
        catalog().authored_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
            used.checked_add(bytes).filter(|total| *total <= AUTHORED_FONT_CACHE_LIMIT)
        }).map_err(|_| "Pinned authored fonts exceed the 256 MiB process limit; restart to release earlier font snapshots".to_string())?;
        Ok(Self(bytes))
    }
}
impl Drop for FontReservation {
    fn drop(&mut self) {
        catalog()
            .authored_bytes
            .fetch_sub(self.0, Ordering::Relaxed);
    }
}
impl AuthoredFont {
    pub fn options(&self) -> resvg::usvg::Options<'static> {
        resvg::usvg::Options {
            fontdb: self.db.clone(),
            ..Default::default()
        }
    }
}
pub(crate) fn authored_font(style: &TextStyle) -> Result<&'static AuthoredFont, String> {
    if let Some(warning) = warning(style) {
        return Err(warning);
    }
    let face = matched(style);
    catalog().authored[&face.id]
        .get_or_init(|| {
            let (data, reservation) = catalog()
                .db
                .with_face_data(face.id, |bytes, _| {
                    let reservation = FontReservation::reserve(bytes.len())?;
                    let mut data = Vec::new();
                    data.try_reserve_exact(bytes.len())
                        .map_err(|_| "Could not allocate the bounded font snapshot".to_string())?;
                    data.extend_from_slice(bytes);
                    Ok::<_, String>((data, reservation))
                })
                .ok_or("Could not read the required font bytes")??;
            let sha256 = format!("{:x}", Sha256::digest(&data));
            let mut db = Database::new();
            db.load_font_data(data);
            let mut pinned = db
                .faces()
                .find(|candidate| candidate.index == face.index)
                .cloned()
                .ok_or("Could not read the required font face index")?;
            if pinned.post_script_name != face.post_script_name {
                return Err("Font PostScript identity changed since font discovery".into());
            }
            // Keep just this face. Fallback to another face is never admitted.
            let ids: Vec<_> = db.faces().map(|candidate| candidate.id).collect();
            for id in ids {
                db.remove_face(id);
            }
            let language = pinned.families[0].1;
            pinned
                .families
                .push((catalog().aliases[&face.id].clone(), language));
            let post_script_name = pinned.post_script_name.clone();
            let index = pinned.index;
            let id = db.push_face_info(pinned);
            // This immutable catalog retains successful snapshots for its
            // lifetime. Failed reads/parses release their reservation via Drop.
            Ok(AuthoredFont {
                id,
                sha256,
                index,
                post_script_name,
                db: Arc::new(db),
                _reservation: reservation,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub(crate) fn families() -> &'static [String] {
    &catalog().families
}
pub(crate) fn family(name: &str) -> Option<&'static str> {
    families()
        .iter()
        .find(|s| s.to_lowercase() == name.trim().to_lowercase())
        .map(String::as_str)
}
#[derive(Clone)]
pub(crate) struct Variant {
    pub face: String,
    pub weight: u16,
    pub italic: bool,
}
impl Variant {
    pub fn label(&self) -> String {
        format!(
            "{} ({})",
            self.face.strip_prefix("WantedSans-").unwrap_or(&self.face),
            self.weight
        )
    }
}
pub(crate) fn variants(name: &str) -> Vec<Variant> {
    let mut variants: Vec<_> = catalog()
        .db
        .faces()
        .filter(|f| f.families.iter().any(|(n, _)| n == name))
        .map(|f| Variant {
            face: f.post_script_name.clone(),
            weight: f.weight.0,
            italic: f.style != Style::Normal,
        })
        .collect();
    variants.sort_by(|a, b| (a.weight, a.italic, &a.face).cmp(&(b.weight, b.italic, &b.face)));
    variants.dedup_by(|a, b| a.face == b.face);
    variants
}
pub(crate) fn style_label(style: &TextStyle) -> String {
    variants(&style.font_family)
        .into_iter()
        .find(|v| v.face == style.font_face)
        .map_or_else(|| label(style.weight, style.italic), |v| v.label())
}
pub(crate) fn label(weight: u16, italic: bool) -> String {
    let name = match weight {
        100 => "Thin",
        200 => "Extra Light",
        300 => "Light",
        400 => "Regular",
        500 => "Medium",
        600 => "Semi Bold",
        700 => "Bold",
        800 => "Extra Bold",
        900 => "Black",
        950 => "Extra Black",
        _ => "Custom",
    };
    format!("{name} ({weight}){}", if italic { " Italic" } else { "" })
}
pub(crate) fn matched(style: &TextStyle) -> &'static resvg::usvg::fontdb::FaceInfo {
    let name = family(&style.font_family).unwrap_or("Wanted Sans");
    let db = &catalog().db;
    if !style.font_face.is_empty() {
        if let Some(face) = db.faces().find(|f| {
            f.post_script_name == style.font_face && f.families.iter().any(|(n, _)| n == name)
        }) {
            return face;
        }
    }
    db.query(&Query {
        families: &[Family::Name(name)],
        weight: Weight(style.weight),
        stretch: Stretch::Normal,
        style: if style.italic {
            Style::Italic
        } else {
            Style::Normal
        },
    })
    .and_then(|id| db.face(id))
    .expect("bundled Wanted Sans is always available")
}
/// Per-face aliases distinguish real faces with identical CSS weight/slant (e.g. Black/ExtraBlack).
pub(crate) fn svg_family(style: &TextStyle) -> &'static str {
    &catalog().aliases[&matched(style).id]
}
pub(crate) fn resolved(style: &TextStyle) -> TextStyle {
    let face = matched(style);
    TextStyle {
        font_family: family(&style.font_family).unwrap_or("Wanted Sans").into(),
        font_face: face.post_script_name.clone(),
        weight: face.weight.0,
        italic: face.style != Style::Normal,
        ..style.clone()
    }
}
pub(crate) fn warning(style: &TextStyle) -> Option<String> {
    let actual = resolved(style);
    if family(&style.font_family).is_none() {
        Some(format!(
            "Missing font: {}. Preview/output use Wanted Sans; the original name is preserved.",
            style.font_family
        ))
    } else if actual.weight != style.weight
        || actual.italic != style.italic
        || (!style.font_face.is_empty() && actual.font_face != style.font_face)
    {
        Some(format!(
            "Style unavailable. Using {}.",
            style_label(&actual)
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_weights_match_real_faces_and_missing_fonts_fall_back() {
        assert_eq!(family(" wanted SANS "), Some("Wanted Sans"));
        assert_eq!(variants("Wanted Sans").len(), 7);
        for variant in variants("Wanted Sans") {
            let style = TextStyle {
                font_face: variant.face,
                weight: variant.weight,
                italic: variant.italic,
                ..Default::default()
            };
            assert_eq!(resolved(&style), style);
            assert_eq!(warning(&style), None);
        }
        let missing = TextStyle {
            font_family: "Libre Effects QA nonexistent font".into(),
            weight: 700,
            ..Default::default()
        };
        let fallback = resolved(&missing);
        assert_eq!(fallback.font_family, "Wanted Sans");
        assert_eq!(fallback.weight, 700);
        assert!(
            warning(&missing)
                .unwrap()
                .contains("original name is preserved")
        );
        assert!(
            warning(&TextStyle {
                italic: true,
                ..Default::default()
            })
            .is_some()
        );
        assert!(Arc::ptr_eq(&database(), &database()));
    }
    #[test]
    fn font_weights_change_pixels_and_roundtrip_through_all_render_paths() {
        use libre_effects_core::{Command, Content, Editor, Project};
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Type QA".into(),
            width: 480,
            height: 180,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Wanted 한글".into(),
                font_size: 48.0,
            },
            width: 420.0,
            height: 100.0,
            name: "Title".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        let renderer = crate::rendering::Renderer::new();
        let regular = renderer.render(e.project(), 0, 480).unwrap();
        let mut seen = vec![regular.clone()];
        for variant in variants("Wanted Sans")
            .into_iter()
            .filter(|v| v.weight != 400)
        {
            let weight = variant.weight;
            e.execute(Command::SetTextStyle {
                id,
                style: TextStyle {
                    weight,
                    font_face: variant.face,
                    ..Default::default()
                },
            })
            .unwrap();
            let frame = renderer.render(e.project(), 0, 480).unwrap();
            assert!(frame.pixels().any(|p| p[3] > 0));
            assert!(
                seen.iter().all(|p| p != &frame),
                "Weight {weight} must render a distinct real face"
            );
            seen.push(frame);
        }
        let bold = seen[3].clone();
        e.execute(Command::SetTextStyle {
            id,
            style: TextStyle {
                font_family: "Missing QA font".into(),
                weight: 700,
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(renderer.render(e.project(), 0, 480).unwrap(), bold);
        let restored = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(renderer.render_preview(&restored, 0, 480).unwrap(), bold);
        let mut png = std::io::Cursor::new(Vec::new());
        renderer
            .render(&restored, 0, 480)
            .unwrap()
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(
            image::load_from_memory(png.get_ref()).unwrap().to_rgba8(),
            bold
        );
        e.undo();
        assert_eq!(
            renderer.render(e.project(), 0, 480).unwrap(),
            *seen.last().unwrap()
        );
        e.redo();
        assert_eq!(renderer.render(e.project(), 0, 480).unwrap(), bold);
    }
}
