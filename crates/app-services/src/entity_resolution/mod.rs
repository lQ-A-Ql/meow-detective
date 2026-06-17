pub mod cross_case;
pub mod merge;
pub mod relationships;

pub use cross_case::{CrossCaseEntityMatcher, CrossCaseMatch, MatchStrategy};
pub use merge::{EntityMergeEngine, ResolvedEntity};
pub use relationships::{EntityRelationship, EntityRelationshipEngine, RelationshipType};
