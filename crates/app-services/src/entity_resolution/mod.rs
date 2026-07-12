//! Entity canonicalization, relationship inference, and cross-case matching.

pub mod cross_case;
mod error;
pub mod merge;
pub mod relationships;

pub use cross_case::{CrossCaseEntityMatcher, CrossCaseMatch, MatchStrategy};
pub use error::EntityResolutionError;
pub use merge::{EntityMergeEngine, ResolvedEntity};
pub use relationships::{EntityRelationship, EntityRelationshipEngine, RelationshipType};
