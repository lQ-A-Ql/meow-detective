//! Read-only parsers for Android system and package-management artifacts.
//!
//! The crate accepts bytes supplied by the caller. It neither opens evidence
//! paths nor writes derived state, keeping evidence access and persistence in
//! the application-service layer.

mod apk;
mod apk_strings;
mod package_list;
mod package_restrictions;
mod package_xml;
mod properties;
mod settings_xml;

pub use apk::{inspect_apk, AndroidApkIcon, AndroidApkPresentation, ApkInspectError};
pub use package_list::{parse_packages_list, AndroidPackageListRecord, PackageListError};
pub use package_restrictions::{
    parse_package_restrictions, AndroidPackageRestriction, PackageRestrictionsError,
};
pub use package_xml::{parse_packages_xml, AndroidPackageMetadata, PackageXmlError};
pub use properties::{parse_build_properties, AndroidBuildProperty, BuildPropertyError};
pub use settings_xml::{find_setting_value, SettingsXmlError};
