mod completion;
mod definition;
mod range;
mod source;

use domain::structure::SourceRevisionRef;
use serde::{Deserialize, Serialize};

use super::{ExecutableRegistry, RuntimeError};
use completion::CompleteBlockOperation;
use range::RegisteredRangeOperation;

pub use source::registered_source_seed;

const ADAPTER: &str = "registered-source";
const VERSION: &str = "registered-source/2";
const FREEZE: &str = "verify-captured-range";
const TARGET: &str = "complete-block-target";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegisteredParameters {
    source: SourceRevisionRef,
    target_size: u64,
}

pub fn registered_registry() -> Result<ExecutableRegistry, RuntimeError> {
    let mut registry = ExecutableRegistry::new(64)?;
    registry.register(Box::new(RegisteredRangeOperation))?;
    registry.register(Box::new(CompleteBlockOperation))?;
    Ok(registry)
}
