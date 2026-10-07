use crate::cpu_clock::{CpuClock, CpuClockKind};
use crate::*;
use rquickjs::{Context, Function, Runtime};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const MAX_LAYERS: usize = 2_048;
const MAX_PROPERTIES: usize = 16_384;
const MAX_MARKERS: usize = 16_384;
const MAX_STRING_BYTES: usize = 16_384;
const MAX_WIRE_BYTES: usize = 4 * 1024 * 1024;
/// Cooperative wall ceiling. The desktop additionally kills the isolated
/// process at its independent two-second deadline, including native VM loops.
pub const MAX_EVALUATION_WALL_TIME: Duration = Duration::from_secs(2);

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

/// Callers may lower budgets. Values above hard ceilings are rejected.
#[derive(Clone, Debug)]
pub struct EvaluationLimits {
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    /// Calling-thread CPU time on Linux, macOS and Windows. Other targets use
    /// an explicitly identified conservative wall-clock fallback.
    pub execution_time: Duration,
    /// Separate monotonic wall ceiling; never an unlimited scheduling allowance.
    pub wall_time: Duration,
    pub max_interrupts: usize,
    pub max_host_reads: usize,
    pub max_expression_evaluations: usize,
    pub max_dependency_depth: usize,
}

impl Default for EvaluationLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 16 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            execution_time: Duration::from_millis(100),
            wall_time: MAX_EVALUATION_WALL_TIME,
            max_interrupts: 10_000,
            max_host_reads: 20_000,
            max_expression_evaluations: 2_048,
            max_dependency_depth: 32,
        }
    }
}

impl EvaluationLimits {
    fn validate(&self) -> Result<(), EvaluationError> {
        if !(1024 * 1024..=64 * 1024 * 1024).contains(&self.memory_bytes)
            || !(64 * 1024..=1024 * 1024).contains(&self.stack_bytes)
            || self.execution_time.is_zero()
            || self.execution_time > Duration::from_secs(2)
            || self.wall_time.is_zero()
            || self.wall_time > MAX_EVALUATION_WALL_TIME
            || !(1..=100_000).contains(&self.max_interrupts)
            || !(1..=100_000).contains(&self.max_host_reads)
            || !(1..=MAX_PROPERTIES).contains(&self.max_expression_evaluations)
            || !(1..=64).contains(&self.max_dependency_depth)
        {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Budget,
                "Invalid expression execution limits",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BudgetStop {
    Execution,
    Wall,
    Interrupts,
}

fn stop_reason(
    execution: Duration,
    wall: Duration,
    interrupts: usize,
    limits: &EvaluationLimits,
) -> Option<BudgetStop> {
    if wall >= limits.wall_time {
        Some(BudgetStop::Wall)
    } else if execution >= limits.execution_time {
        Some(BudgetStop::Execution)
    } else if interrupts > limits.max_interrupts {
        Some(BudgetStop::Interrupts)
    } else {
        None
    }
}

struct ExecutionBudget {
    clock: CpuClock,
    wall_started: Instant,
    limits: EvaluationLimits,
    interrupts: Cell<usize>,
    stopped: Cell<Option<BudgetStop>>,
    clock_error: RefCell<Option<String>>,
    phase: Cell<&'static str>,
    last_execution: Cell<Duration>,
    last_wall: Cell<Duration>,
}
impl ExecutionBudget {
    fn new(limits: &EvaluationLimits) -> Result<Self, EvaluationError> {
        let wall_started = Instant::now();
        let clock = CpuClock::start().map_err(runtime_error)?;
        Ok(Self {
            clock,
            wall_started,
            limits: limits.clone(),
            interrupts: Cell::new(0),
            stopped: Cell::new(None),
            clock_error: RefCell::new(None),
            phase: Cell::new("runtime setup"),
            last_execution: Cell::new(Duration::ZERO),
            last_wall: Cell::new(Duration::ZERO),
        })
    }
    fn phase(&self, phase: &'static str) {
        if self.stopped.get().is_none() {
            self.phase.set(phase);
        }
    }
    fn poll(&self, interrupt: bool) -> bool {
        if self.stopped.get().is_some() || self.clock_error.borrow().is_some() {
            return true;
        }
        if interrupt {
            self.interrupts.set(self.interrupts.get().saturating_add(1));
        }
        let execution = match self.clock.elapsed() {
            Ok(elapsed) => elapsed,
            Err(error) => {
                *self.clock_error.borrow_mut() = Some(error);
                return true;
            }
        };
        let wall = self.wall_started.elapsed();
        self.last_execution.set(execution);
        self.last_wall.set(wall);
        let reason = stop_reason(execution, wall, self.interrupts.get(), &self.limits);
        self.stopped.set(reason);
        reason.is_some()
    }
    fn finish(&self) -> Result<(), EvaluationError> {
        self.poll(false);
        if let Some(error) = self.clock_error.borrow().as_ref() {
            return Err(runtime_error(error));
        }
        let message = match self.stopped.get() {
            None => return Ok(()),
            Some(BudgetStop::Execution) => match self.clock.kind() {
                CpuClockKind::ThreadCpu => {
                    "Expression execution budget exceeded (active evaluator CPU)"
                }
                CpuClockKind::WallFallback => {
                    "Expression execution budget exceeded (conservative wall-clock fallback)"
                }
            },
            Some(BudgetStop::Wall) => "Expression wall-time budget exceeded",
            Some(BudgetStop::Interrupts) => "Expression interrupt-count budget exceeded",
        };
        Err(EvaluationError::new(
            EvaluationErrorKind::Budget,
            format!(
                "{message}; phase={}, charged={:.3} ms, wall={:.3} ms, interrupts={}",
                self.phase.get(),
                self.last_execution.get().as_secs_f64() * 1000.0,
                self.last_wall.get().as_secs_f64() * 1000.0,
                self.interrupts.get(),
            ),
        ))
    }
}

#[derive(Default)]
pub struct ExpressionEvaluator {
    pub limits: EvaluationLimits,
    pub cancel: Arc<AtomicBool>,
}

impl ExpressionEvaluator {
    /// Evaluate synchronously on a worker, not the editor UI/render thread.
    /// Failure returns no partially successful view. Callers must display the
    /// diagnostic and explicitly choose any authored-value fallback policy.
    pub fn evaluate(
        &self,
        snapshot: &CompositionSnapshot,
        requested: &[PropertyAddress],
    ) -> Result<EvaluatedProperties, EvaluationError> {
        self.limits.validate()?;
        if self.cancel.load(Ordering::Relaxed) {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Canceled,
                "Expression evaluation canceled",
            ));
        }
        let (wire, addresses) = prepare(snapshot, requested, &self.limits)?;
        let input = serialize_wire(&wire)?;
        // Charge all VM setup, host work and guest execution to this calling
        // thread; descheduling does not spend its active-work allowance. A
        // separate wall limit and the external process watchdog remain active.
        let budget = Rc::new(ExecutionBudget::new(&self.limits)?);
        let check_budget = || {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(EvaluationError::new(
                    EvaluationErrorKind::Canceled,
                    "Expression evaluation canceled",
                ));
            }
            budget.finish()
        };
        let asynchronous = Rc::new(Cell::new(false));
        let runtime = Runtime::new();
        check_budget()?;
        let runtime = runtime.map_err(runtime_error)?;
        runtime.set_memory_limit(self.limits.memory_bytes);
        runtime.set_max_stack_size(self.limits.stack_bytes);
        let async_flag = asynchronous.clone();
        runtime.set_promise_hook(Some(Box::new(move |_, _, _, _| async_flag.set(true))));
        let interrupt_budget = budget.clone();
        let cancel = self.cancel.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            cancel.load(Ordering::Relaxed) || interrupt_budget.poll(true)
        })));
        budget.phase("context setup");
        let context = Context::full(&runtime);
        check_budget()?;
        let context = context.map_err(runtime_error)?;
        let output = context.with(|ctx| -> Result<String, EvaluationError> {
            budget.phase("host compilation");
            let evaluate = ctx
                .eval::<Function, _>(include_str!("host.js"))
                .map_err(|error| javascript_error(&ctx, error))?;
            budget.phase("host and expressions");
            evaluate
                .call((input,))
                .map_err(|error| javascript_error(&ctx, error))
        });
        check_budget()?;
        budget.phase("result validation");
        if asynchronous.get() || runtime.is_job_pending() {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Unsupported,
                "Asynchronous expressions are unsupported",
            ));
        }
        let output = output?;
        if output.len() > MAX_WIRE_BYTES {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Budget,
                "Expression result exceeds 4 MiB",
            ));
        }
        let output: WireOutput = serde_json::from_str(&output).map_err(runtime_error)?;
        if let Some(error) = output.error {
            return Err(EvaluationError {
                kind: error.kind,
                message: error.message,
                property: error
                    .property
                    .and_then(|index| addresses.get(index).cloned()),
            });
        }
        let mut values = BTreeMap::new();
        let mut dependencies = BTreeMap::new();
        for result in output.values {
            let address = addresses
                .get(result.property)
                .ok_or_else(|| runtime_error("Invalid result address"))?
                .clone();
            if !result.value.is_valid()
                || !result
                    .value
                    .same_kind(wire.properties[result.property].value)
            {
                return Err(EvaluationError::new(
                    EvaluationErrorKind::InvalidResult,
                    "Expression result has an invalid type or nonfinite component",
                ));
            }
            let deps = result
                .dependencies
                .iter()
                .map(|index| {
                    addresses
                        .get(*index)
                        .cloned()
                        .ok_or_else(|| runtime_error("Invalid dependency address"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if values.insert(address.clone(), result.value).is_some() {
                return Err(runtime_error("Duplicate evaluated property address"));
            }
            dependencies.insert(address, deps);
        }
        if requested
            .iter()
            .any(|address| !values.contains_key(address))
            || dependencies
                .values()
                .flatten()
                .any(|address| !values.contains_key(address))
            || output.host_reads > self.limits.max_host_reads
            || output.expression_evaluations > self.limits.max_expression_evaluations
        {
            return Err(runtime_error(
                "Incomplete or invalid evaluated-property view",
            ));
        }
        let result = EvaluatedProperties {
            composition: snapshot.id,
            time: snapshot.time,
            values,
            dependencies,
            expression_evaluations: output.expression_evaluations,
            host_reads: output.host_reads,
        };
        // Validation/address cloning/result construction are host work too.
        // Never publish a successful view after cancellation or exhaustion.
        check_budget()?;
        Ok(result)
    }
}

#[derive(Serialize)]
struct WireInput<'a> {
    width: u32,
    height: u32,
    duration: f64,
    time: f64,
    frame_rate: FrameRate,
    sources: &'a [String],
    layers: Vec<WireLayer<'a>>,
    properties: Vec<WireProperty<'a>>,
    requested: Vec<usize>,
    max_host_reads: usize,
    max_expression_evaluations: usize,
    max_dependency_depth: usize,
}

#[derive(Serialize)]
struct WireLayer<'a> {
    name: &'a str,
    start_time: f64,
    in_point: f64,
    out_point: f64,
    position: usize,
    scale: usize,
    opacity: usize,
    source_text: Option<usize>,
    sliders: Vec<(&'a str, usize)>,
    markers: &'a [MarkerSnapshot],
}

#[derive(Serialize)]
struct WireProperty<'a> {
    layer: usize,
    name: String,
    value: &'a PropertyValue,
    source_id: Option<ExpressionSourceId>,
    local_bindings: &'a [String],
}

/// Stop JSON escaping/encoding at the bridge limit rather than constructing an
/// unbounded temporary string and checking its size after the allocation.
struct BoundedWireWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for BoundedWireWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_WIRE_BYTES - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("Expression snapshot exceeds 4 MiB"));
        }
        let required = self.bytes.len() + bytes.len();
        if required > self.bytes.capacity() {
            let capacity = required
                .max(self.bytes.capacity().saturating_mul(2))
                .min(MAX_WIRE_BYTES);
            self.bytes.reserve_exact(capacity - self.bytes.len());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialize_wire(wire: &WireInput<'_>) -> Result<String, EvaluationError> {
    let mut writer = BoundedWireWriter {
        bytes: Vec::with_capacity(1024),
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, wire) {
        return Err(if writer.exceeded {
            EvaluationError::new(
                EvaluationErrorKind::Budget,
                "Expression snapshot exceeds 4 MiB",
            )
        } else {
            runtime_error(error)
        });
    }
    String::from_utf8(writer.bytes).map_err(runtime_error)
}

#[derive(Deserialize)]
struct WireOutput {
    error: Option<WireError>,
    values: Vec<WireValue>,
    expression_evaluations: usize,
    host_reads: usize,
}

#[derive(Deserialize)]
struct WireError {
    kind: EvaluationErrorKind,
    message: String,
    property: Option<usize>,
}

#[derive(Deserialize)]
struct WireValue {
    property: usize,
    value: PropertyValue,
    dependencies: Vec<usize>,
}

fn prepare<'a>(
    snapshot: &'a CompositionSnapshot,
    requested: &[PropertyAddress],
    limits: &EvaluationLimits,
) -> Result<(WireInput<'a>, Vec<PropertyAddress>), EvaluationError> {
    let invalid = |message| EvaluationError::new(EvaluationErrorKind::InvalidSnapshot, message);
    if snapshot.width == 0
        || snapshot.height == 0
        || snapshot.frame_rate.numerator == 0
        || snapshot.frame_rate.denominator == 0
        || !snapshot.duration.is_finite()
        || snapshot.duration <= 0.0
        || !snapshot.time.is_finite()
        || snapshot.layers.len() > MAX_LAYERS
        || requested.len() > MAX_PROPERTIES
    {
        return Err(invalid(
            "Invalid composition dimensions, clock, or snapshot size",
        ));
    }
    if snapshot.sources.len() > MAX_EXPRESSION_SOURCES {
        return Err(EvaluationError::new(
            EvaluationErrorKind::Budget,
            "Expression source table exceeds 16384 entries",
        ));
    }
    let mut unique_sources = BTreeSet::new();
    let mut total_source = 0_usize;
    for source in &snapshot.sources {
        total_source = total_source.saturating_add(source.len());
        if source.len() > MAX_EXPRESSION_SOURCE_BYTES
            || total_source > MAX_TOTAL_EXPRESSION_SOURCE_BYTES
        {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Budget,
                "Expression source budget exceeded",
            ));
        }
        if !unique_sources.insert(source.as_str()) {
            return Err(invalid("Duplicate expression source table entry"));
        }
    }
    let mut ids = BTreeSet::new();
    let mut addresses = Vec::new();
    let mut properties = Vec::new();
    let mut layers = Vec::new();
    let mut total_markers = 0_usize;
    let mut total_strings = 0_usize;
    for (layer_index, layer) in snapshot.layers.iter().enumerate() {
        total_markers = total_markers.saturating_add(layer.markers.len());
        if layer.sliders.len() > MAX_PROPERTIES
            || layer.masks.len() > MAX_PROPERTIES
            || total_markers > MAX_MARKERS
        {
            return Err(invalid("Layer controls or markers exceed snapshot budgets"));
        }
        total_strings = total_strings.saturating_add(layer.name.len());
        for marker in &layer.markers {
            total_strings = total_strings.saturating_add(marker.comment.len());
        }
        for slider in &layer.sliders {
            total_strings = total_strings.saturating_add(slider.name.len());
        }
        if total_strings > 1024 * 1024 {
            return Err(invalid("Snapshot text exceeds 1 MiB"));
        }
        if !ids.insert(layer.id)
            || layer.name.len() > MAX_STRING_BYTES
            || !layer.start_time.is_finite()
            || !layer.in_point.is_finite()
            || !layer.out_point.is_finite()
            || layer.out_point < layer.in_point
            || layer.sliders.len() > MAX_PROPERTIES
            || total_markers > MAX_MARKERS
            || layer
                .markers
                .iter()
                .any(|marker| !marker.time.is_finite() || marker.comment.len() > MAX_STRING_BYTES)
            || layer
                .markers
                .windows(2)
                .any(|pair| pair[0].time >= pair[1].time)
        {
            return Err(invalid("Invalid layer identity, timing, markers, or size"));
        }
        let mut add = |property: &ExpressionProperty,
                       sample: &'a PropertySnapshot|
         -> Result<usize, EvaluationError> {
            let valid_kind = matches!(
                (property, &sample.authored_value),
                (
                    ExpressionProperty::Opacity | ExpressionProperty::Slider(_),
                    PropertyValue::Scalar(_)
                ) | (
                    ExpressionProperty::Position | ExpressionProperty::Scale,
                    PropertyValue::Vector2(_) | PropertyValue::Vector3(_)
                ) | (ExpressionProperty::SourceText, PropertyValue::Text(_))
                    | (ExpressionProperty::MaskPath(_), PropertyValue::Path(_))
            );
            if !sample.authored_value.is_valid() || !valid_kind {
                return Err(invalid("Invalid authored property type or nonfinite value"));
            }
            if let PropertyValue::Text(text) = &sample.authored_value {
                total_strings = total_strings.saturating_add(text.len());
                if total_strings > 1024 * 1024 {
                    return Err(invalid("Snapshot text exceeds 1 MiB"));
                }
            }
            let source_id = match &sample.expression {
                Some(program) => {
                    // Validate even disabled/unrequested bindings. A source ID
                    // only has meaning inside this snapshot's validated table.
                    if program.source_id.0 as usize >= snapshot.sources.len() {
                        return Err(invalid(
                            "Expression source ID is absent from the source table",
                        ));
                    }
                    validate_local_bindings(&program.local_bindings).map_err(|message| {
                        EvaluationError::new(EvaluationErrorKind::InvalidSnapshot, message)
                    })?;
                    program.enabled.then_some(program.source_id)
                }
                None => None,
            };
            if properties.len() >= MAX_PROPERTIES {
                return Err(EvaluationError::new(
                    EvaluationErrorKind::Budget,
                    "Expression property budget exceeded",
                ));
            }
            let index = properties.len();
            addresses.push(PropertyAddress {
                composition: snapshot.id,
                layer: layer.id,
                property: property.clone(),
            });
            properties.push(WireProperty {
                layer: layer_index,
                name: format!("{property:?}"),
                value: &sample.authored_value,
                source_id,
                local_bindings: sample
                    .expression
                    .as_ref()
                    .map_or(&[], |program| program.local_bindings.as_slice()),
            });
            Ok(index)
        };
        let position = add(&ExpressionProperty::Position, &layer.position)?;
        let scale = add(&ExpressionProperty::Scale, &layer.scale)?;
        let opacity = add(&ExpressionProperty::Opacity, &layer.opacity)?;
        let source_text = layer
            .source_text
            .as_ref()
            .map(|text| add(&ExpressionProperty::SourceText, text))
            .transpose()?;
        let mut mask_ids = BTreeSet::new();
        for mask in &layer.masks {
            if !mask_ids.insert(mask.id) {
                return Err(invalid("Duplicate mask expression identity"));
            }
            add(&ExpressionProperty::MaskPath(mask.id), &mask.property)?;
        }
        let mut names = BTreeSet::new();
        let mut sliders = Vec::new();
        for slider in &layer.sliders {
            if slider.name.len() > MAX_STRING_BYTES || !names.insert(&slider.name) {
                return Err(invalid("Duplicate or oversized slider effect name"));
            }
            sliders.push((
                slider.name.as_str(),
                add(
                    &ExpressionProperty::Slider(slider.name.clone()),
                    &slider.property,
                )?,
            ));
        }
        layers.push(WireLayer {
            name: &layer.name,
            start_time: layer.start_time,
            in_point: layer.in_point,
            out_point: layer.out_point,
            position,
            scale,
            opacity,
            source_text,
            sliders,
            markers: &layer.markers,
        });
    }
    let indices: BTreeMap<_, _> = addresses
        .iter()
        .enumerate()
        .map(|(index, address)| (address, index))
        .collect();
    let requested = requested
        .iter()
        .map(|address| {
            indices
                .get(address)
                .copied()
                .ok_or_else(|| EvaluationError {
                    kind: EvaluationErrorKind::MissingReference,
                    message:
                        "Requested expression property is absent from this composition snapshot"
                            .into(),
                    property: Some(address.clone()),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((
        WireInput {
            width: snapshot.width,
            height: snapshot.height,
            duration: snapshot.duration,
            time: snapshot.time,
            frame_rate: snapshot.frame_rate,
            sources: &snapshot.sources,
            layers,
            properties,
            requested,
            max_host_reads: limits.max_host_reads,
            max_expression_evaluations: limits.max_expression_evaluations,
            max_dependency_depth: limits.max_dependency_depth,
        },
        addresses,
    ))
}

fn runtime_error(error: impl std::fmt::Display) -> EvaluationError {
    EvaluationError::new(EvaluationErrorKind::Runtime, error.to_string())
}

fn javascript_error(ctx: &rquickjs::Ctx<'_>, error: rquickjs::Error) -> EvaluationError {
    let message = if error.is_exception() {
        let exception = ctx.catch();
        if let Some(object) = exception.as_object() {
            object
                .get::<_, String>("message")
                .unwrap_or_else(|_| error.to_string())
        } else {
            error.to_string()
        }
    } else {
        error.to_string()
    };
    EvaluationError::new(
        EvaluationErrorKind::JavaScript,
        message.chars().take(2048).collect::<String>(),
    )
}
