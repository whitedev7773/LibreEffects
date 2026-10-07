//! Deliberately bounded, static SVG -> editable Contents conversion.
//! Preflight owns the SVG contract; usvg receives only sanitized geometry, never
//! arbitrary source XML, styles, references or executable/resource-bearing data.
use libre_effects_core::{
    Affine, ContentsKind, ContentsNode, ContentsParam, PathVertex, Property, ShapeContents,
    ShapeParam, ShapeStroke, StrokeCap, StrokeJoin, VectorPath,
};
use resvg::{tiny_skia, usvg};
use roxmltree::Node;
use std::{collections::BTreeSet, io::Read, path::Path, str::FromStr};

pub const MAX_BYTES: usize = 1024 * 1024;
const MAX_XML_NODES: u32 = 2048;
const MAX_ELEMENTS: usize = 512;
const MAX_SEGMENTS: usize = 8192;
const MAX_STYLE_BYTES: usize = 16 * 1024;
const MAX_STYLE_DECLARATIONS: usize = 128;
const MAX_DOCUMENT_DECLARATIONS: usize = 2048;
#[path = "svg_import_gradient.rs"]
mod gradient;
use gradient::{Gradients, Paint};

const SVG_NS: &str = "http://www.w3.org/2000/svg";

#[derive(Debug)]
pub struct ImportedSvg {
    pub contents: ShapeContents,
    pub width: f64,
    pub height: f64,
}

pub fn read_svg_file(path: &Path) -> Result<ImportedSvg, String> {
    let before = std::fs::symlink_metadata(path).map_err(|e| format!("Cannot inspect SVG: {e}"))?;
    if !before.file_type().is_file() {
        return Err("SVG import requires a regular local file (no symlinks)".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Prevent a path replacement from following a symlink or blocking on a
        // FIFO/device between the preliminary check and the handle check.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: open a reparse point itself, not its target.
        options.custom_flags(0x00200000).share_mode(0x00000001);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("Cannot open SVG: {e}"))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("Cannot inspect SVG: {e}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x00000400 != 0 {
            return Err("SVG import does not follow reparse points".into());
        }
    }
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("SVG import requires a regular local file".into());
    }
    if meta.len() > MAX_BYTES as u64 {
        return Err("SVG exceeds the 1 MiB import limit".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read SVG: {e}"))?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<ImportedSvg, String> {
    if bytes.len() > MAX_BYTES {
        return Err("SVG exceeds the 1 MiB import limit".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG must be uncompressed UTF-8 XML")?;
    validate_declaration(text)?;
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES,
        },
    )
    .map_err(|e| format!("Unsafe or invalid SVG XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err("Expected an outer svg element".into());
    }
    let mut budget = Budget::default();
    for n in doc.descendants() {
        if n.is_pi() {
            return Err("SVG processing instructions are unsupported".into());
        }
        if n.is_element() {
            budget.elements += 1;
            if budget.elements > MAX_ELEMENTS {
                return Err("SVG exceeds the 512-element limit".into());
            }
            if n.ancestors().filter(Node::is_element).count() > 8 {
                return Err("SVG nesting exceeds the supported depth".into());
            }
            validate_element(n, n == root, &mut budget)?;
        } else if n.is_text()
            && !n.text().unwrap_or("").trim().is_empty()
            && !n
                .parent_element()
                .is_some_and(|p| matches!(p.tag_name().name(), "title" | "desc"))
        {
            return Err("Text content is unsupported outside title/desc".into());
        }
    }
    let viewbox = root.attribute("viewBox").map(number_list).transpose()?;
    let viewbox = match viewbox {
        Some(v) if v.len() == 4 && v[2] > 0. && v[3] > 0. => Some([v[0], v[1], v[2], v[3]]),
        Some(_) => return Err("viewBox requires four finite numbers and positive size".into()),
        None => None,
    };
    let dimension = |name, index| -> Result<f64, String> {
        let v = match root.attribute(name) {
            Some(s) => length(s)?,
            None => viewbox
                .map(|v| v[index])
                .ok_or_else(|| format!("SVG needs {name} or viewBox"))?,
        };
        if !(1. ..=16384.).contains(&v) || v.fract() != 0. {
            return Err(format!(
                "SVG {name} must be an integer from 1..16384 pixels; fractional viewport clipping is unsupported"
            ));
        }
        Ok(v)
    };
    let (width, height) = (dimension("width", 2)?, dimension("height", 3)?);
    let gradients = Gradients::parse(
        root,
        viewbox.map(|v| [v[2], v[3]]).unwrap_or([width, height]),
    )?;
    // Resolve every supplied reference, including shadowed and unused paint.
    for node in doc.descendants().filter(Node::is_element) {
        if !matches!(
            node.tag_name().name(),
            "defs" | "linearGradient" | "radialGradient" | "stop" | "title" | "desc"
        ) {
            Style::default().inherited(node, &gradients)?;
        }
    }
    let style = Style::default().inherited(root, &gradients)?;
    let mut children = convert_children(root, &style, &gradients, &mut budget)?;
    if children.is_empty() {
        return Err("SVG has no editable painted geometry".into());
    }
    if let Some(v) = viewbox {
        let aspect = root
            .attribute("preserveAspectRatio")
            .unwrap_or("xMidYMid meet");
        let transform = viewport_transform(v, width, height, aspect)?;
        children = vec![group("ViewBox", children, transform, 1.)?];
    } else if root.attribute("preserveAspectRatio").is_some() {
        return Err("preserveAspectRatio requires viewBox".into());
    }
    let opacity = style.local_opacity;
    // Root transforms are intentionally unsupported: SVG 2 root transform semantics
    // differ from SVG 1.1 viewBox placement. The preflight rejects that attribute.
    let items = vec![group(
        root.attribute("id").unwrap_or("SVG"),
        children,
        Affine::default(),
        opacity,
    )?];
    let contents = ShapeContents::from_nodes(items)
        .map_err(|e| format!("SVG cannot fit editable Contents: {e}"))?;
    Ok(ImportedSvg {
        contents,
        width,
        height,
    })
}

fn validate_declaration(text: &str) -> Result<(), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text
        .strip_prefix("<?xml")
        .is_some_and(|tail| tail.starts_with(char::is_whitespace))
    {
        let declaration = text.split_once("?>").ok_or("Unclosed XML declaration")?.0;
        let attrs = declaration.strip_prefix("<?xml").unwrap();
        let source = format!("<declaration{attrs}/>");
        let doc = roxmltree::Document::parse(&source).map_err(|_| "Invalid XML declaration")?;
        let root = doc.root_element();
        if root.attribute("version") != Some("1.0") {
            return Err("Only XML version 1.0 is supported".into());
        }
        for attribute in root.attributes() {
            match attribute.name() {
                "version" => {}
                "encoding" if attribute.value().eq_ignore_ascii_case("utf-8") => {}
                "standalone" if matches!(attribute.value(), "yes" | "no") => {}
                _ => return Err("Unsupported XML declaration; use XML 1.0 / UTF-8".into()),
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Budget {
    elements: usize,
    segments: usize,
    style_declarations: usize,
    ids: BTreeSet<String>,
}
impl Budget {
    fn segment(&mut self) -> Result<(), String> {
        self.segments += 1;
        if self.segments > MAX_SEGMENTS {
            return Err("SVG exceeds the 8192 path-segment work limit".into());
        }
        Ok(())
    }
}
const PAINT_ATTRS: &[&str] = &[
    "fill",
    "stroke",
    "fill-rule",
    "fill-opacity",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
];
fn geometry_attrs(tag: &str) -> &'static [&'static str] {
    match tag {
        "path" => &["d"],
        "rect" => &["x", "y", "width", "height", "rx", "ry"],
        "circle" => &["cx", "cy", "r"],
        "ellipse" => &["cx", "cy", "rx", "ry"],
        "line" => &["x1", "y1", "x2", "y2"],
        "polyline" | "polygon" => &["points"],
        _ => &[],
    }
}
fn validate_element(n: Node<'_, '_>, root: bool, budget: &mut Budget) -> Result<(), String> {
    let tag = n.tag_name().name();
    if n.tag_name().namespace().is_some_and(|ns| ns != SVG_NS) {
        return Err(format!("Unsupported namespace on <{tag}>"));
    }
    if !matches!(
        tag,
        "svg"
            | "g"
            | "path"
            | "rect"
            | "circle"
            | "ellipse"
            | "line"
            | "polyline"
            | "polygon"
            | "title"
            | "desc"
            | "defs"
            | "linearGradient"
            | "radialGradient"
            | "stop"
    ) || (tag == "svg" && !root)
    {
        return Err(format!("Unsupported SVG element <{tag}>"));
    }
    if matches!(tag, "defs" | "linearGradient" | "radialGradient" | "stop") {
        return gradient::validate_element(n, budget);
    }
    if matches!(tag, "title" | "desc") && n.children().any(|c| c.is_element()) {
        return Err(format!("<{tag}> must contain only inert text"));
    }
    if !matches!(tag, "svg" | "g" | "title" | "desc")
        && n.children()
            .any(|c| c.is_element() && !matches!(c.tag_name().name(), "title" | "desc"))
    {
        return Err(format!("Nested geometry inside <{tag}> is unsupported"));
    }
    for a in n.attributes() {
        let name = a.name();
        let valid = a.namespace().is_none()
            && (name == "id"
                || (!matches!(tag, "title" | "desc")
                    && (PAINT_ATTRS.contains(&name) || name == "style"))
                || (!root && !matches!(tag, "title" | "desc") && name == "transform")
                || geometry_attrs(tag).contains(&name)
                || (root
                    && matches!(
                        name,
                        "width" | "height" | "viewBox" | "preserveAspectRatio" | "version"
                    )));
        if !valid {
            return Err(format!("Unsupported SVG attribute '{name}' on <{tag}>"));
        }
        if name == "id"
            && (a.value().trim().is_empty()
                || a.value().chars().count() > 128
                || !budget.ids.insert(a.value().to_owned()))
        {
            return Err("SVG IDs must be nonempty, unique and at most 128 characters".into());
        }
        if name == "style" {
            budget.style_declarations += inline_declarations(a.value())?.len();
            if budget.style_declarations > MAX_DOCUMENT_DECLARATIONS {
                return Err("SVG exceeds the 2048 inline-declaration document limit".into());
            }
        }
        if name == "version" && !matches!(a.value(), "1.0" | "1.1" | "2.0") {
            return Err("Unsupported SVG version".into());
        }
    }
    Ok(())
}
fn check_commas(s: &str, transform: bool) -> Result<(), String> {
    let mut rest = s;
    while let Some(index) = rest.find(',') {
        rest = &rest[index + 1..];
        let next = rest.trim_start().chars().next();
        if !next.is_some_and(|c| {
            c.is_ascii_digit()
                || matches!(c, '+' | '-' | '.')
                || (transform && c.is_ascii_alphabetic())
        }) {
            return Err("Invalid trailing or empty SVG comma separator".into());
        }
    }
    Ok(())
}
fn number(s: &str) -> Result<f64, String> {
    let values = number_list(s)?;
    if values.len() != 1 {
        return Err("Expected one unitless SVG number".into());
    }
    Ok(values[0])
}
fn strict_list(s: &str, percent: bool) -> Result<Vec<(f64, bool)>, String> {
    check_commas(s, false)?;
    if s.trim_start().starts_with(',') {
        return Err("Leading SVG comma separator".into());
    }
    let mut result = Vec::new();
    for token in s
        .split(|c| matches!(c, ',' | ' ' | '\t' | '\r' | '\n'))
        .filter(|token| !token.is_empty())
    {
        let (token, is_percent) = match token.strip_suffix('%') {
            Some(number) if percent => (number, true),
            _ => (token, false),
        };
        let value = svgtypes::Number::from_str(token)
            .map_err(|_| "SVG number lists require complete comma/whitespace-separated numbers")?
            .0;
        finite(value)?;
        result.push((value, is_percent));
        if result.len() > MAX_SEGMENTS * 2 {
            return Err("SVG numeric list exceeds the work limit".into());
        }
    }
    Ok(result)
}
fn number_list(s: &str) -> Result<Vec<f64>, String> {
    Ok(strict_list(s, false)?
        .into_iter()
        .map(|(number, _)| number)
        .collect())
}
fn finite(v: f64) -> Result<(), String> {
    if !v.is_finite() || v.abs() > 1_000_000. || (v != 0. && (v as f32) == 0.) {
        Err("SVG value exceeds the finite f32-representable ±1000000 range".into())
    } else {
        Ok(())
    }
}
fn length(s: &str) -> Result<f64, String> {
    let v = svgtypes::Length::from_str(s).map_err(|e| format!("Invalid SVG length: {e}"))?;
    if !matches!(
        v.unit,
        svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px
    ) {
        return Err("Only unitless/px SVG lengths are supported".into());
    }
    finite(v.number)?;
    Ok(v.number)
}
fn opacity(s: &str) -> Result<f64, String> {
    let v = s.trim().parse::<f64>().map_err(|_| "Invalid SVG opacity")?;
    if !v.is_finite() || !(0. ..=1.).contains(&v) {
        return Err("SVG opacity must be between 0 and 1".into());
    }
    Ok(v)
}
fn color(s: &str) -> Result<Option<svgtypes::Color>, String> {
    let source = s.trim();
    if source == "none" {
        return Ok(None);
    }
    let lower = source.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix('#') {
        if !matches!(hex.len(), 3 | 6) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(
                "SVG solid colors support 3/6-digit RGB hex, without embedded alpha".into(),
            );
        }
    } else if let Some(rgb) = lower.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        check_commas(rgb, false)?;
        let channels = strict_list(rgb, true)?;
        if channels.len() != 3 {
            return Err("SVG RGB colors require exactly three channels".into());
        }
        let percent = channels[0].1;
        if channels.iter().any(|(value, unit)| {
            *unit != percent || !(0. ..=if percent { 100. } else { 255. }).contains(value)
        }) {
            return Err(
                "SVG RGB channels require matching unitless 0..255 or percent 0..100 values".into(),
            );
        }
    } else if !lower.chars().all(|c| c.is_ascii_alphabetic()) || lower.is_empty() {
        return Err("Only named, RGB and 3/6-digit hex solid SVG colors are supported".into());
    }
    let c = svgtypes::Color::from_str(source)
        .map_err(|_| "Only literal solid SVG colors or none are supported")?;
    if c.alpha != 255 {
        return Err("Alpha color syntax is unsupported; use fill/stroke-opacity".into());
    }
    Ok(Some(c))
}
#[derive(Clone)]
struct Style {
    fill: Paint,
    stroke: Paint,
    even_odd: bool,
    fill_opacity: f64,
    stroke_opacity: f64,
    stroke_width: f64,
    stroke_style: ShapeStroke,
    // Element compositing is local, unlike every other field in this structure.
    local_opacity: f64,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Paint::Solid(svgtypes::Color::new_rgb(0, 0, 0)),
            stroke: Paint::None,
            even_odd: false,
            fill_opacity: 1.,
            stroke_opacity: 1.,
            stroke_width: 1.,
            local_opacity: 1.,
            stroke_style: ShapeStroke {
                join: StrokeJoin::Miter,
                ..Default::default()
            },
        }
    }
}
impl Style {
    fn inherited(&self, n: Node<'_, '_>, gradients: &Gradients) -> Result<Self, String> {
        let mut v = self.clone();
        // SVG opacity is not inherited: it composites this element's subtree.
        v.local_opacity = 1.;
        // Presentation attributes have lower precedence regardless of XML order.
        // Validate even values that an inline declaration will later override.
        for a in n.attributes().filter(|a| PAINT_ATTRS.contains(&a.name())) {
            v.apply(a.name(), a.value(), gradients)?;
        }
        if let Some(source) = n.attribute("style") {
            for (name, value) in inline_declarations(source)? {
                v.apply(&name, &value, gradients)?;
            }
        }
        Ok(v)
    }

    fn apply(&mut self, name: &str, value: &str, gradients: &Gradients) -> Result<(), String> {
        match name {
            "fill" => self.fill = gradients.paint(value)?,
            "stroke" => self.stroke = gradients.paint(value)?,
            "fill-opacity" => self.fill_opacity = opacity(value)?,
            "stroke-opacity" => self.stroke_opacity = opacity(value)?,
            "opacity" => self.local_opacity = opacity(value)?,
            "stroke-width" => self.stroke_width = length(value)?,
            "fill-rule" => {
                self.even_odd = match value {
                    "evenodd" => true,
                    "nonzero" => false,
                    _ => return Err("Unsupported fill-rule".into()),
                }
            }
            "stroke-linecap" => {
                self.stroke_style.cap = match value {
                    "butt" => StrokeCap::Butt,
                    "round" => StrokeCap::Round,
                    "square" => StrokeCap::Square,
                    _ => return Err("Unsupported stroke-linecap".into()),
                }
            }
            "stroke-linejoin" => {
                self.stroke_style.join = match value {
                    "miter" => StrokeJoin::Miter,
                    "round" => StrokeJoin::Round,
                    "bevel" => StrokeJoin::Bevel,
                    _ => return Err("Unsupported stroke-linejoin".into()),
                }
            }
            "stroke-miterlimit" => self.stroke_style.miter_limit = number(value)?,
            "stroke-dashoffset" => self.stroke_style.dash_offset = length(value)?,
            "stroke-dasharray" => {
                if value.trim().is_empty() {
                    return Err("Empty SVG stroke-dasharray is invalid".into());
                }
                self.stroke_style.dashes = if value.trim() == "none" {
                    vec![]
                } else {
                    number_list(value)?
                }
            }
            _ => return Err(format!("Unsupported SVG presentation property '{name}'")),
        }
        // Validate each declaration independently, including shadowed ones.
        if !(0. ..=1024.).contains(&self.stroke_width) || !self.stroke_style.valid() {
            return Err("SVG stroke exceeds editable width/miter/dash bounds".into());
        }
        Ok(())
    }
}

/// A closed declaration language, not a forgiving CSS parser. The lexical pass
/// rejects CSS quoting/escaping/comment/resource constructs before delimiters
/// are interpreted. Only literal rgb() and local url(#ID) paint tokens are admitted.
/// This keeps arbitrary CSS away from the geometry-only usvg normalizer.
fn inline_declarations(source: &str) -> Result<Vec<(String, String)>, String> {
    declarations(source, PAINT_ATTRS)
}
fn declarations(source: &str, allowed: &[&str]) -> Result<Vec<(String, String)>, String> {
    if source.len() > MAX_STYLE_BYTES {
        return Err("SVG inline style exceeds the 16 KiB element limit".into());
    }
    let mut declarations = Vec::new();
    let mut start = 0;
    let mut in_parentheses = false;
    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'(' if !in_parentheses => in_parentheses = true,
            b')' if in_parentheses => in_parentheses = false,
            b'(' | b')' => return Err("Nested or unmatched SVG style parentheses".into()),
            b';' if !in_parentheses => {
                inline_declaration(&source[start..index], allowed, &mut declarations)?;
                start = index + 1;
            }
            b':' if !in_parentheses => {}
            b';' | b':' => return Err("SVG style delimiters inside a function are unsupported".into()),
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9'
            | b'-' | b'+' | b'.' | b',' | b'%' | b'#' | b'_'
            | b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' => {}
            _ => return Err("SVG inline styles support only literal ASCII declarations; comments, escapes, strings, resources and !important are unsupported".into()),
        }
    }
    if in_parentheses {
        return Err("Unclosed SVG style function".into());
    }
    inline_declaration(&source[start..], allowed, &mut declarations)?;
    Ok(declarations)
}

fn inline_declaration(
    source: &str,
    allowed: &[&str],
    out: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let source = source.trim_ascii();
    if source.is_empty() {
        return Ok(());
    }
    if out.len() == MAX_STYLE_DECLARATIONS {
        return Err("SVG inline style exceeds the 128-declaration element limit".into());
    }
    let (name, value) = source
        .split_once(':')
        .ok_or("SVG style declaration needs a colon")?;
    let name = name.trim_ascii().to_ascii_lowercase();
    if !allowed.contains(&name.as_str()) {
        return Err(format!("Unsupported SVG inline property '{name}'"));
    }
    let value = value.trim_ascii();
    // Fragment IDs are case-sensitive even though CSS keywords are not.
    let value = if value
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("url("))
    {
        format!("url({}", &value[4..])
    } else {
        value.to_ascii_lowercase()
    };
    if value.is_empty() || value.contains(':') {
        return Err("SVG style requires one complete nonempty value per declaration".into());
    }
    if value.contains(['(', ')']) {
        let rgb = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')'));
        let local =
            matches!(name.as_str(), "fill" | "stroke") && gradient::reference(&value).is_ok();
        if !local
            && (!matches!(name.as_str(), "fill" | "stroke" | "stop-color")
                || !rgb.is_some_and(|v| !v.contains(['(', ')'])))
        {
            return Err(
                "SVG inline functions require literal three-channel rgb() or local paint url(#ID)"
                    .into(),
            );
        }
    }
    // CSS ASCII whitespace includes form feed; normalize it for the existing
    // complete SVG value parsers, without changing any numeric token boundaries.
    let value = value.replace(['\t', '\r', '\n', '\x0c'], " ");
    validate_inline_numbers(&name, &value)?;
    out.push((name, value));
    Ok(())
}

/// CSS numbers are stricter than SVG path numbers (notably `1.` is not a
/// number token). Check the closed inline subset before the pinned SVG parsers.
fn inline_number(source: &str) -> Result<(), String> {
    let bytes = source.as_bytes();
    let mut pos = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = pos;
    while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
        pos += 1;
    }
    let integer_digits = pos - start;
    if bytes.get(pos) == Some(&b'.') {
        pos += 1;
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos == start {
            return Err("SVG inline CSS numbers require digits after a decimal point".into());
        }
    } else if integer_digits == 0 {
        return Err("Invalid SVG inline CSS number".into());
    }
    let mantissa_end = pos;
    if bytes.get(pos) == Some(&b'e') {
        pos += 1;
        if matches!(bytes.get(pos), Some(b'+' | b'-')) {
            pos += 1;
        }
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos == start {
            return Err("Invalid SVG inline CSS exponent".into());
        }
    }
    if pos != bytes.len() {
        return Err("SVG inline CSS numbers require complete numeric tokens".into());
    }
    let value = source
        .parse::<f64>()
        .map_err(|_| "Invalid SVG inline CSS number")?;
    finite(value)?;
    if value == 0.
        && bytes[..mantissa_end]
            .iter()
            .any(|b| matches!(b, b'1'..=b'9'))
    {
        return Err("SVG inline CSS number underflows the supported range".into());
    }
    Ok(())
}

fn validate_inline_numbers(name: &str, value: &str) -> Result<(), String> {
    match name {
        "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" | "stroke-miterlimit" => {
            inline_number(value)
        }
        "stroke-width" | "stroke-dashoffset" => {
            inline_number(value.strip_suffix("px").unwrap_or(value))
        }
        "stroke-dasharray" if value != "none" => {
            for token in value
                .split(|c: char| c == ',' || c.is_ascii_whitespace())
                .filter(|s| !s.is_empty())
            {
                inline_number(token)?;
            }
            Ok(())
        }
        "fill" | "stroke" | "stop-color" if value.starts_with("rgb(") => {
            let channels = &value[4..value.len() - 1];
            let tokens: Vec<_> = if channels.contains(',') {
                channels.split(',').map(str::trim_ascii).collect()
            } else {
                channels.split_ascii_whitespace().collect()
            };
            if tokens.len() != 3 {
                return Err("SVG inline RGB requires three comma-separated or three space-separated channels".into());
            }
            for token in tokens {
                inline_number(token.strip_suffix('%').unwrap_or(token))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn convert_children(
    n: Node<'_, '_>,
    style: &Style,
    gradients: &Gradients,
    budget: &mut Budget,
) -> Result<Vec<ContentsNode>, String> {
    let mut out = vec![];
    for child in n.children().filter(Node::is_element) {
        if matches!(child.tag_name().name(), "title" | "desc" | "defs") {
            continue;
        }
        out.push(convert(child, style, gradients, budget)?);
    }
    out.reverse(); // Contents earlier groups composite in front; SVG later siblings do.
    Ok(out)
}
fn convert(
    n: Node<'_, '_>,
    inherited: &Style,
    gradients: &Gradients,
    budget: &mut Budget,
) -> Result<ContentsNode, String> {
    let style = inherited.inherited(n, gradients)?;
    let transform = parse_transform(n.attribute("transform"))?;
    let alpha = style.local_opacity;
    let tag = n.tag_name().name();
    let children = if tag == "g" {
        convert_children(n, &style, gradients, budget)?
    } else {
        let (paths, bounds) = geometry(n, budget)?;
        let mut nodes = paths
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                let mut node = ContentsNode::with_defaults(ContentsKind::Path {
                    path,
                    animation: Default::default(),
                });
                node.name = format!("Path {}", i + 1);
                node
            })
            .collect::<Vec<_>>();
        if let Some(paint) = gradients.node(&style.stroke, true, &style, bounds)? {
            nodes.push(paint);
        }
        if let Some(paint) = gradients.node(&style.fill, false, &style, bounds)? {
            nodes.push(paint);
        }
        if matches!(style.fill, Paint::None) && matches!(style.stroke, Paint::None) {
            return Err("Unpainted SVG geometry is unsupported in this import subset".into());
        }
        nodes
    };
    if children.is_empty() {
        return Err("Empty SVG groups are unsupported".into());
    }
    group(n.attribute("id").unwrap_or(tag), children, transform, alpha)
}
fn parse_transform(source: Option<&str>) -> Result<Affine, String> {
    let Some(s) = source else {
        return Ok(Affine::default());
    };
    if s.trim().is_empty() {
        return Err("Empty SVG transform is invalid".into());
    }
    check_commas(s, true)?;
    for arguments in s.split('(').skip(1) {
        let arguments = arguments.split_once(')').ok_or("Unclosed SVG transform")?.0;
        if number_list(arguments)?.is_empty() {
            return Err("Empty SVG transform arguments".into());
        }
    }
    let t = svgtypes::Transform::from_str(s).map_err(|e| format!("Invalid SVG transform: {e}"))?;
    let t = Affine([t.a, t.b, t.c, t.d, t.e, t.f]);
    for value in t.0 {
        finite(value)?;
    }
    if (t.0[0] * t.0[3] - t.0[1] * t.0[2]).abs() <= 1e-12 {
        return Err("Singular SVG transforms are unsupported".into());
    }
    Ok(t)
}
fn group(
    name: &str,
    children: Vec<ContentsNode>,
    t: Affine,
    alpha: f64,
) -> Result<ContentsNode, String> {
    let [a, b, c, d, e, f] = t.0;
    for v in t.0 {
        finite(v)?;
    }
    let sx = a.hypot(b);
    let determinant = a * d - b * c;
    if sx <= 1e-12 || determinant.abs() <= 1e-12 {
        return Err("Singular SVG transforms are unsupported".into());
    }
    let sy = determinant / sx;
    let shear = (a * c + b * d) / (sx * sy);
    let mut node = ContentsNode::with_defaults(ContentsKind::Group(children));
    node.name = name.to_owned();
    for (p, v) in [
        (ContentsParam::Transform(Property::PositionX), e),
        (ContentsParam::Transform(Property::PositionY), f),
        (ContentsParam::Transform(Property::ScaleX), sx * 100.),
        (ContentsParam::Transform(Property::ScaleY), sy * 100.),
        (
            ContentsParam::Transform(Property::Rotation),
            b.atan2(a).to_degrees(),
        ),
        (ContentsParam::Skew, -shear.atan().to_degrees()),
        (ContentsParam::Transform(Property::Opacity), alpha * 100.),
    ] {
        node.set_static_value(p, v)
            .map_err(|_| "SVG transform exceeds editable scale/skew/position bounds")?;
    }
    // Decomposition must preserve the input affine numerically. Never silently
    // clamp a skew or transform that the existing model cannot represent.
    if node
        .transform(0)
        .0
        .iter()
        .zip(t.0)
        .any(|(x, y)| (x - y).abs() > 1e-9 * (1. + y.abs()))
    {
        return Err("SVG transform cannot be represented faithfully".into());
    }
    Ok(node)
}
fn viewport_transform(v: [f64; 4], w: f64, h: f64, s: &str) -> Result<Affine, String> {
    // svgtypes accepts a trailing token here, so validate the complete grammar first.
    let words = s.split_whitespace().collect::<Vec<_>>();
    if words.is_empty()
        || !matches!(
            words[0],
            "none"
                | "xMinYMin"
                | "xMidYMin"
                | "xMaxYMin"
                | "xMinYMid"
                | "xMidYMid"
                | "xMaxYMid"
                | "xMinYMax"
                | "xMidYMax"
                | "xMaxYMax"
        )
        || words.len() > 2
        || (words.len() == 2 && !matches!(words[1], "meet" | "slice"))
    {
        return Err("Invalid preserveAspectRatio".into());
    }
    let ratio = svgtypes::AspectRatio::from_str(s).map_err(|_| "Invalid preserveAspectRatio")?;
    let (mut sx, mut sy) = (w / v[2], h / v[3]);
    if ratio.align != svgtypes::Align::None {
        let scale = if ratio.slice { sx.max(sy) } else { sx.min(sy) };
        sx = scale;
        sy = scale;
    }
    use svgtypes::Align::*;
    let ax = match ratio.align {
        XMidYMin | XMidYMid | XMidYMax => 0.5,
        XMaxYMin | XMaxYMid | XMaxYMax => 1.,
        _ => 0.,
    };
    let ay = match ratio.align {
        XMinYMid | XMidYMid | XMaxYMid => 0.5,
        XMinYMax | XMidYMax | XMaxYMax => 1.,
        _ => 0.,
    };
    Ok(Affine([
        sx,
        0.,
        0.,
        sy,
        -v[0] * sx + (w - v[2] * sx) * ax,
        -v[1] * sy + (h - v[3] * sy) * ay,
    ]))
}
fn geometry(
    n: Node<'_, '_>,
    budget: &mut Budget,
) -> Result<(Vec<VectorPath>, Option<tiny_skia::Rect>), String> {
    let tag = n.tag_name().name();
    let mut attrs = String::new();
    for &name in geometry_attrs(tag) {
        if let Some(raw) = n.attribute(name) {
            match name {
                "d" => {
                    if raw.trim().is_empty() {
                        return Err("SVG path data is empty".into());
                    }
                    check_commas(raw, false)?;
                    use svgtypes::PathSegment::*;
                    let mut previous_close = false;
                    let mut moved = false;
                    let mut drawable = false;
                    for part in svgtypes::PathParser::from(raw) {
                        budget.segment()?;
                        let segment = part.map_err(|e| format!("Malformed SVG path: {e}"))?;
                        let closed = matches!(segment, ClosePath { .. });
                        if closed && previous_close {
                            return Err("Repeated SVG close commands are unsupported".into());
                        }
                        previous_close = closed;
                        if matches!(segment, MoveTo { .. }) {
                            if moved && !drawable {
                                return Err("Move-only SVG contours are unsupported".into());
                            }
                            moved = true;
                            drawable = false;
                        } else if !closed {
                            drawable = true;
                        }
                        let values = match segment {
                            MoveTo { x, y, .. }
                            | LineTo { x, y, .. }
                            | SmoothQuadratic { x, y, .. } => vec![x, y],
                            HorizontalLineTo { x, .. } => vec![x],
                            VerticalLineTo { y, .. } => vec![y],
                            CurveTo {
                                x1,
                                y1,
                                x2,
                                y2,
                                x,
                                y,
                                ..
                            } => vec![x1, y1, x2, y2, x, y],
                            SmoothCurveTo { x2, y2, x, y, .. } => vec![x2, y2, x, y],
                            Quadratic { x1, y1, x, y, .. } => vec![x1, y1, x, y],
                            EllipticalArc {
                                rx,
                                ry,
                                x_axis_rotation,
                                x,
                                y,
                                ..
                            } => vec![rx, ry, x_axis_rotation, x, y],
                            ClosePath { .. } => vec![],
                        };
                        for v in values {
                            finite(v)?;
                        }
                    }
                    if !drawable {
                        return Err("Move-only SVG contours are unsupported".into());
                    }
                    for part in svgtypes::SimplifyingPathParser::from(raw) {
                        budget.segment()?;
                        use svgtypes::SimplePathSegment as P;
                        let values = match part.map_err(|e| format!("Malformed SVG path: {e}"))? {
                            P::MoveTo { x, y } | P::LineTo { x, y } => vec![x, y],
                            P::CurveTo {
                                x1,
                                y1,
                                x2,
                                y2,
                                x,
                                y,
                            } => vec![x1, y1, x2, y2, x, y],
                            P::Quadratic { x1, y1, x, y } => vec![x1, y1, x, y],
                            P::ClosePath => vec![],
                        };
                        for v in values {
                            finite(v)?;
                        }
                    }
                    attrs.push_str(&format!(" d='{}'", xml_attr(raw)));
                }
                "points" => {
                    let points = number_list(raw)?;
                    if points.len() < 4 || points.len() % 2 != 0 {
                        return Err("SVG points need complete coordinate pairs".into());
                    }
                    for _ in points.chunks(2) {
                        budget.segment()?;
                    }
                    attrs.push_str(&format!(
                        " points='{}'",
                        points
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
                _ => {
                    let value = length(raw)?;
                    if matches!(name, "width" | "height" | "rx" | "ry" | "r") && value < 0. {
                        return Err(format!("Negative SVG {name} is unsupported"));
                    }
                    attrs.push_str(&format!(" {name}='{value}'"));
                }
            }
        }
    }
    // Only already-whitelisted geometry with completely validated values reaches
    // usvg. No source attributes/children/styles/IDs/hrefs enter this document.
    let source = format!(
        "<svg xmlns='{SVG_NS}' width='16384' height='16384'><{tag}{attrs} fill='black' stroke='black' stroke-width='1'/></svg>"
    );
    let tree = usvg::Tree::from_str(&source, &usvg::Options::default())
        .map_err(|e| format!("Cannot normalize SVG geometry: {e}"))?;
    fn only_path(g: &usvg::Group) -> Result<Option<&tiny_skia::Path>, String> {
        let mut found = None;
        for child in g.children() {
            let next = match child {
                usvg::Node::Path(p) => Some(p.data()),
                usvg::Node::Group(g) => only_path(g)?,
                _ => return Err("Unexpected SVG normalization output".into()),
            };
            if next.is_some() && found.is_some() {
                return Err("Unexpected multiple normalized geometries".into());
            }
            if next.is_some() {
                found = next;
            }
        }
        Ok(found)
    }
    let data = only_path(tree.root())?
        .ok_or("SVG geometry is empty or degenerate and cannot be imported")?;
    Ok((
        paths_from_geometry(data, budget)?,
        data.compute_tight_bounds(),
    ))
}
fn xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn paths_from_geometry(
    data: &tiny_skia::Path,
    budget: &mut Budget,
) -> Result<Vec<VectorPath>, String> {
    let mut paths = vec![];
    let mut path = VectorPath::default();
    let point = |p: tiny_skia::Point| [p.x as f64, p.y as f64];
    let difference = |a: [f64; 2], b: [f64; 2]| [a[0] - b[0], a[1] - b[1]];
    fn finish(path: &mut VectorPath, out: &mut Vec<VectorPath>) -> Result<(), String> {
        if path.vertices.is_empty() {
            return Ok(());
        }
        // Retain the explicit endpoint when closure repeats the first anchor.
        // Zero-length closing cubics are faithful and keep legal 2-arc contours.
        if path.closed && path.vertices.len() == 2 {
            // Two-vertex closed curves are legal SVG, but core needs three.
            // Split the first cubic exactly; no shape or winding changes.
            if !path.insert(0, 0.5) {
                return Err("Cannot split two-point closed SVG contour".into());
            }
        }
        if !path.valid() {
            return Err(
                "SVG contour exceeds editable 2/3..1024 vertex or coordinate bounds".into(),
            );
        }
        out.push(std::mem::take(path));
        Ok(())
    }
    use tiny_skia::PathSegment::*;
    for segment in data.segments() {
        budget.segment()?;
        match segment {
            MoveTo(p) => {
                finish(&mut path, &mut paths)?;
                path.vertices.push(PathVertex::corner(point(p)));
            }
            LineTo(p) => {
                let p = point(p);
                let previous = path
                    .vertices
                    .last_mut()
                    .ok_or("SVG line has no starting point")?;
                let delta = [
                    (p[0] - previous.position[0]) / 3.,
                    (p[1] - previous.position[1]) / 3.,
                ];
                previous.outgoing = delta;
                path.vertices.push(PathVertex {
                    position: p,
                    incoming: [-delta[0], -delta[1]],
                    outgoing: [0.; 2],
                });
            }
            QuadTo(c, p) => {
                let p = point(p);
                let c = point(c);
                let prev = path
                    .vertices
                    .last_mut()
                    .ok_or("SVG quadratic has no starting point")?;
                prev.outgoing = [
                    (c[0] - prev.position[0]) * 2. / 3.,
                    (c[1] - prev.position[1]) * 2. / 3.,
                ];
                path.vertices.push(PathVertex {
                    position: p,
                    incoming: [(c[0] - p[0]) * 2. / 3., (c[1] - p[1]) * 2. / 3.],
                    outgoing: [0.; 2],
                });
            }
            CubicTo(c1, c2, p) => {
                let p = point(p);
                let prev = path
                    .vertices
                    .last_mut()
                    .ok_or("SVG cubic has no starting point")?;
                prev.outgoing = difference(point(c1), prev.position);
                path.vertices.push(PathVertex {
                    position: p,
                    incoming: difference(point(c2), p),
                    outgoing: [0.; 2],
                });
            }
            Close => {
                if let (Some(first), Some(last)) = (path.vertices.first(), path.vertices.last()) {
                    let delta = [
                        (first.position[0] - last.position[0]) / 3.,
                        (first.position[1] - last.position[1]) / 3.,
                    ];
                    path.vertices.last_mut().unwrap().outgoing = delta;
                    path.vertices.first_mut().unwrap().incoming = [-delta[0], -delta[1]];
                }
                path.closed = true;
                finish(&mut path, &mut paths)?;
            }
        }
        if path.vertices.len() > 1024 {
            return Err("SVG contour exceeds 1024 vertices".into());
        }
    }
    finish(&mut path, &mut paths)?;
    if paths.is_empty() {
        return Err("SVG has no editable contours".into());
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_reader_rejects_directories_large_files_and_links() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_svg_file(dir.path()).is_err());
        let path = dir.path().join("valid.svg");
        std::fs::write(
            &path,
            b"<svg width='20' height='20'><rect width='10' height='10'/></svg>",
        )
        .unwrap();
        assert!(read_svg_file(&path).is_ok());
        let huge = dir.path().join("huge.svg");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len((MAX_BYTES + 1) as u64)
            .unwrap();
        assert!(read_svg_file(&huge).is_err());
        #[cfg(unix)]
        {
            let link = dir.path().join("linked.svg");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(read_svg_file(&link).is_err());
            use std::os::unix::ffi::OsStrExt;
            let fifo = dir.path().join("pipe.svg");
            let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
            // SAFETY: a valid NUL-terminated path inside this isolated temporary directory.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            assert!(read_svg_file(&fifo).is_err());
        }
    }
    #[test]
    fn rejects_complete_grammar_failures_without_normalizer_fallback() {
        for attrs in [
            "width='2cm' height='40'",
            "width='40oops' height='40'",
            "width='40' height='NaN'",
            "viewBox='0 0 40 40 2'",
            "viewBox='0 0 40 40' preserveAspectRatio='xMidYMid meet garbage'",
        ] {
            assert!(
                parse(format!("<svg {attrs}><rect width='10' height='10'/></svg>").as_bytes())
                    .is_err(),
                "{attrs}"
            );
        }
        for content in [
            "<path d='M0 0L20 20 garbage'/>",
            "<path d='M0 0L1e300 0'/>",
            "<polyline points='0 0 10 10 junk'/>",
            "<rect width='0' height='10'/>",
            "<g transform='scale(0)'><rect width='10' height='10'/></g>",
        ] {
            assert!(
                parse(format!("<svg width='40' height='40'>{content}</svg>").as_bytes()).is_err(),
                "{content}"
            );
        }
    }
    #[test]
    fn no_resource_or_script_documents_enter_normalizer() {
        for content in [
            "<script>alert(1)</script>",
            "<image href='file:///tmp/x'/>",
            "<use href='#a'/>",
            "<rect width='10' height='10' onload='alert(1)'/>",
            "<rect width='10' height='10' style='fill:url(https://example.com/x)'/>",
            "<rect width='10' height='10' fill='url(https://example.com/x)'/>",
            "<?load href='https://example.com'?><rect width='10' height='10'/>",
        ] {
            assert!(
                parse(format!("<svg width='40' height='40'>{content}</svg>").as_bytes()).is_err(),
                "{content}"
            );
        }
        assert!(parse(b"<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><svg width='40' height='40'><title>&x;</title></svg>").is_err());
    }
    #[test]
    fn imported_tree_keeps_paths_and_paints_with_finite_defaults() {
        let parsed=parse(b"<svg xmlns='http://www.w3.org/2000/svg' width='100' height='80'><g opacity='.5' transform='translate(4 5) scale(1.5 .5)'><rect id='Box' width='30' height='20' fill='#ff8800' stroke='blue' stroke-width='2'/><path d='M1 2q4 5 8 9t6 7' fill='none' stroke='black'/></g></svg>").unwrap();
        assert_eq!((parsed.width, parsed.height), (100., 80.));
        parsed.contents.validate(90).unwrap();
        assert!(
            parsed
                .contents
                .rows()
                .iter()
                .any(|(_, _, n)| n.name == "Box")
        );
    }
}
