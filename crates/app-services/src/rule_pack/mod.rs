//! Rule pack engine: parse, validate, and execute TOML-based correlation rule packs.
//!
//! ## Module structure
//!
//! - [`parser`] — TOML deserialization types and the `parse_rule_pack` entry point.
//! - [`validator`] — Semantic validation of parsed rule packs.
//! - [`engine`] — Execute rule packs against a case database to create graph edges.
//!
//! ## Quick start
//!
//! ```ignore
//! use app_services::rule_pack::{parser, validator, engine};
//!
//! let toml_str = std::fs::read_to_string("v2-standard.toml")?;
//! let pack = parser::parse_rule_pack(&toml_str)?;
//! let errors = validator::validate_rule_pack(&pack);
//! if !errors.is_empty() {
//!     return Err(errors.join("\n"));
//! }
//! let count = engine::execute_rule_pack(&conn, "case-1", &pack)?;
//! ```

pub mod engine;
pub mod parser;
pub mod validator;

// Re-export key types for convenience
pub use engine::{execute_rule_pack, execute_rule_pack_incremental};
pub use parser::{parse_rule_pack, RuleDefinition, RulePack, RulePackManifest, V2_STANDARD_TOML};
pub use validator::validate_rule_pack;
