//! A read-only, synchronous AE expression subset, independent of core and GPUI.
//!
//! Call on a worker with a detached snapshot sampled at one composition time.
//! The result is a transient evaluated-property view, never an authored track.
//! No project mutation, files, network, UI, module loader or jobs are exposed.
//! This is a capability-restricted embedded VM, not an OS security sandbox.
mod cpu_clock;
mod model;
mod runtime;

pub use model::*;
pub use runtime::{EvaluationLimits, ExpressionEvaluator, MAX_EVALUATION_WALL_TIME};

#[cfg(test)]
mod tests;
