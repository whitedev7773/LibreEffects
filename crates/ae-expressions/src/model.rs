use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Hard ceilings for a detached snapshot's private source table.
pub const MAX_EXPRESSION_SOURCES: usize = 16_384;
pub const MAX_EXPRESSION_SOURCE_BYTES: usize = 65_536;
pub const MAX_TOTAL_EXPRESSION_SOURCE_BYTES: usize = 524_288;
pub const MAX_EXPRESSION_TEXT_BYTES: usize = 16_384;
pub const MAX_EXPRESSION_PATH_VERTICES: usize = 1_024;
pub const MAX_EXPRESSION_PATH_COORDINATE: f64 = 1_000_000.0;
pub const MAX_EXPRESSION_LOCAL_BINDINGS: usize = 64;
pub const MAX_EXPRESSION_LOCAL_BINDING_BYTES: usize = 64;

/// Composition IDs cannot be confused with layer, footage or folder IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CompositionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LayerId(pub u64);

/// Index into one detached snapshot's exact-byte source table, not a project ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExpressionSourceId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ExpressionProperty {
    Position,
    Scale,
    Opacity,
    /// The exact, case-sensitive effect instance name, not its match name.
    Slider(String),
    SourceText,
    /// Stable native mask identity. Mask reads are not part of this subset.
    MaskPath(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyAddress {
    pub composition: CompositionId,
    pub layer: LayerId,
    pub property: ExpressionProperty,
}

/// Values use AE units: pixels, scale percentages, opacity percentages.
/// A three-component snapshot is not evidence of a native 3D renderer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Scalar(f64),
    Vector2([f64; 2]),
    Vector3([f64; 3]),
    Text(String),
    Path(ExpressionPath),
}

/// Layer-local vertices and relative incoming/outgoing Bezier handles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPath {
    pub vertices: Vec<[f64; 2]>,
    pub in_tangents: Vec<[f64; 2]>,
    pub out_tangents: Vec<[f64; 2]>,
    pub closed: bool,
}

impl ExpressionPath {
    pub fn is_valid(&self) -> bool {
        self.vertices.len() >= if self.closed { 3 } else { 2 }
            && self.vertices.len() <= MAX_EXPRESSION_PATH_VERTICES
            && self.in_tangents.len() == self.vertices.len()
            && self.out_tangents.len() == self.vertices.len()
            && self
                .vertices
                .iter()
                .chain(&self.in_tangents)
                .chain(&self.out_tangents)
                .flatten()
                .all(|value| value.is_finite() && value.abs() <= MAX_EXPRESSION_PATH_COORDINATE)
    }
}

impl PropertyValue {
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Scalar(value) => value.is_finite(),
            Self::Vector2(value) => value.iter().all(|v| v.is_finite()),
            Self::Vector3(value) => value.iter().all(|v| v.is_finite()),
            Self::Text(_) => true,
            Self::Path(value) => value
                .vertices
                .iter()
                .chain(&value.in_tangents)
                .chain(&value.out_tangents)
                .flatten()
                .all(|v| v.is_finite()),
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Text(value) => value.len() <= MAX_EXPRESSION_TEXT_BYTES,
            Self::Path(value) => value.is_valid(),
            _ => self.is_finite(),
        }
    }

    pub(crate) fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionProgram {
    pub source_id: ExpressionSourceId,
    pub enabled: bool,
    /// Explicit compatibility adaptation: fresh lexical locals around unchanged
    /// strict direct-eval source. Empty retains the original strict behavior.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_bindings: Vec<String>,
}

/// Validate generated wrapper identifiers before compiling any guest source.
/// Host/global names cannot be shadowed by this compatibility metadata.
pub fn validate_local_bindings(bindings: &[String]) -> Result<(), String> {
    if bindings.len() > MAX_EXPRESSION_LOCAL_BINDINGS {
        return Err("Expression local bindings exceed 64 names".into());
    }
    let mut unique = std::collections::BTreeSet::new();
    for name in bindings {
        let mut bytes = name.bytes();
        let first = bytes.next();
        if name.len() > MAX_EXPRESSION_LOCAL_BINDING_BYTES
            || !first.is_some_and(|c| c.is_ascii_alphabetic() || c == b'_' || c == b'$')
            || !bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
            || !unique.insert(name)
            || RESERVED_BINDINGS.contains(&name.as_str())
        {
            return Err("Expression local bindings must be unique, nonreserved ASCII identifiers of at most 64 bytes".into());
        }
    }
    Ok(())
}

// Includes strict/future-reserved words, wrapper bindings, and the pinned VM's
// global capabilities. The host additionally checks its actual global names.
const RESERVED_BINDINGS: &[&str] = &[
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "eval",
    "arguments",
    "thisComp",
    "thisLayer",
    "time",
    "inPoint",
    "outPoint",
    "startTime",
    "value",
    "framesToTime",
    "transform",
    "marker",
    "effect",
    "linear",
    "createPath",
    "globalThis",
    "Infinity",
    "NaN",
    "undefined",
    "Object",
    "Function",
    "Boolean",
    "Symbol",
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "InternalError",
    "Number",
    "BigInt",
    "Math",
    "Date",
    "String",
    "RegExp",
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "BigInt64Array",
    "BigUint64Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Atomics",
    "JSON",
    "WeakRef",
    "FinalizationRegistry",
    "Iterator",
    "Promise",
    "Proxy",
    "Reflect",
    "DisposableStack",
    "AsyncDisposableStack",
    "SuppressedError",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "escape",
    "unescape",
    "app",
    "File",
    "Folder",
    "Socket",
    "system",
    "$",
    "Window",
    "alert",
    "confirm",
    "fetch",
    "XMLHttpRequest",
    "setTimeout",
    "setInterval",
    "constructor",
    "__proto__",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toString",
    "toLocaleString",
    "valueOf",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertySnapshot {
    /// Pre-expression track sample at `CompositionSnapshot::time`.
    pub authored_value: PropertyValue,
    pub expression: Option<ExpressionProgram>,
}

impl PropertySnapshot {
    pub fn authored(authored_value: PropertyValue) -> Self {
        Self {
            authored_value,
            expression: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliderSnapshot {
    pub name: String,
    pub property: PropertySnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskSnapshot {
    pub id: u64,
    pub property: PropertySnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerSnapshot {
    /// Composition seconds, not time relative to the layer's in point.
    pub time: f64,
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerSnapshot {
    pub id: LayerId,
    pub name: String,
    /// Independent signed composition-time origin; never inferred from trim.
    pub start_time: f64,
    pub in_point: f64,
    pub out_point: f64,
    pub position: PropertySnapshot,
    pub scale: PropertySnapshot,
    pub opacity: PropertySnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_text: Option<PropertySnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masks: Vec<MaskSnapshot>,
    pub sliders: Vec<SliderSnapshot>,
    /// Strictly increasing composition times. One-based key lookup preserves order.
    pub markers: Vec<MarkerSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    pub fn frames_to_time(self, frames: f64) -> f64 {
        frames * f64::from(self.denominator) / f64::from(self.numerator)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionSnapshot {
    pub id: CompositionId,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub frame_rate: FrameRate,
    pub time: f64,
    /// Required private transport table. Entries preserve exact UTF-8 bytes;
    /// duplicates and oversized/unused entries are validated before execution.
    /// Authored native projects continue to store each expression's own source.
    pub sources: Vec<String>,
    /// AE stack order, topmost first. Duplicate names resolve to the first layer.
    pub layers: Vec<LayerSnapshot>,
}

/// Contains requested properties and dependencies actually read at this time.
/// Each expression runs at most once, even when multiple consumers reference it.
/// Rebuild on time/document changes: there is intentionally no cross-frame cache.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluatedProperties {
    pub composition: CompositionId,
    pub time: f64,
    /// IPC encodes maps as address/value pairs; JSON object keys cannot carry
    /// typed addresses. Duplicate addresses reject rather than overwrite.
    #[serde(with = "address_map")]
    pub values: BTreeMap<PropertyAddress, PropertyValue>,
    #[serde(with = "address_map")]
    pub dependencies: BTreeMap<PropertyAddress, Vec<PropertyAddress>>,
    pub expression_evaluations: usize,
    pub host_reads: usize,
}

impl EvaluatedProperties {
    pub fn get(&self, address: &PropertyAddress) -> Option<&PropertyValue> {
        self.values.get(address)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationErrorKind {
    InvalidSnapshot,
    Unsupported,
    MissingReference,
    Cycle,
    InvalidResult,
    JavaScript,
    Budget,
    Canceled,
    Runtime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationError {
    pub kind: EvaluationErrorKind,
    pub message: String,
    pub property: Option<PropertyAddress>,
}

impl std::fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for EvaluationError {}

impl EvaluationError {
    pub(crate) fn new(kind: EvaluationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            property: None,
        }
    }
}

/// Sequence encoding keeps typed addresses and full-width IDs without string-key
/// adapters. This is a transport representation, not an authored-project format.
mod address_map {
    use super::PropertyAddress;
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;
    use std::fmt;
    use std::marker::PhantomData;

    const MAX_ENTRIES: usize = 16_384;

    pub fn serialize<S, V>(
        map: &BTreeMap<PropertyAddress, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        if map.len() > MAX_ENTRIES {
            return Err(serde::ser::Error::custom(
                "Expression result map exceeds its entry budget",
            ));
        }
        let mut sequence = serializer.serialize_seq(Some(map.len()))?;
        for (address, value) in map {
            sequence.serialize_element(&(address, value))?;
        }
        sequence.end()
    }

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<BTreeMap<PropertyAddress, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        struct MapVisitor<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for MapVisitor<V> {
            type Value = BTreeMap<PropertyAddress, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded sequence of unique property-address/value pairs")
            }

            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut map = BTreeMap::new();
                while let Some((address, value)) =
                    sequence.next_element::<(PropertyAddress, V)>()?
                {
                    if map.len() >= MAX_ENTRIES {
                        return Err(A::Error::custom(
                            "Expression result map exceeds its entry budget",
                        ));
                    }
                    if map.insert(address, value).is_some() {
                        return Err(A::Error::custom("Duplicate expression property address"));
                    }
                }
                Ok(map)
            }
        }
        deserializer.deserialize_seq(MapVisitor(PhantomData))
    }
}
