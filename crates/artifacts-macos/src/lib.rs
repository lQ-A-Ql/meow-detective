//! macOS artifact parsers for the Forensics Workbench.
//!
//! This crate provides read-only parsers for common macOS forensic artifacts:
//! FSEvents, Launch Services, property lists (binary/XML), quarantine events,
//! recent items, Spotlight metadata, and the unified log tracev3 format.
//!
//! All public entry points return [`Result<T>`](self::Result) backed by the
//! typed [`MacArtifactError`] enum instead of raw strings.

pub mod error;
pub mod fsevents;
pub mod launch_services;
pub mod plist;
pub mod quarantine;
pub mod recent_items;
pub mod spotlight;
pub mod unified_log;
pub(crate) mod xml;

pub use error::{MacArtifactError, Result};
pub use fsevents::{parse_fsevents_log, FSEvent, FSEventType};
pub use launch_services::{parse_launch_services_plist, LaunchService};
pub use plist::{parse_binary_plist, parse_xml_plist, MacPlistEntry, PlistType};
pub use quarantine::{parse_quarantine_events, QuarantineEntry};
pub use recent_items::{parse_recent_items_plist, RecentItem, RecentItemKind};
pub use spotlight::{parse_spotlight_store, SpotlightEntry};
pub use unified_log::{parse_tracev3, UnifiedLogEntry};
