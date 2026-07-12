use super::context::RegistryExtractionContext;
use super::extractors::{amcache, ntuser, sam, security, software, system, usrclass};

enum HiveKind {
    System,
    Software,
    Sam,
    Ntuser,
    Usrclass,
    Amcache,
    Security,
    Other,
}

pub(super) fn extract(normalized_path: &str, mut context: RegistryExtractionContext<'_>) {
    match classify(normalized_path) {
        HiveKind::System => system::extract(&mut context),
        HiveKind::Software => software::extract(&mut context),
        HiveKind::Sam => sam::extract(&mut context),
        HiveKind::Ntuser => ntuser::extract(&mut context),
        HiveKind::Usrclass => usrclass::extract(&mut context),
        HiveKind::Amcache => amcache::extract(&mut context),
        HiveKind::Security => security::extract(&mut context),
        HiveKind::Other => {}
    }
}

fn classify(normalized_path: &str) -> HiveKind {
    if normalized_path.ends_with("/windows/system32/config/system") {
        HiveKind::System
    } else if normalized_path.ends_with("/windows/system32/config/software") {
        HiveKind::Software
    } else if normalized_path.ends_with("/windows/system32/config/sam") {
        HiveKind::Sam
    } else if normalized_path.ends_with("/ntuser.dat") {
        HiveKind::Ntuser
    } else if normalized_path.ends_with("/usrclass.dat") {
        HiveKind::Usrclass
    } else if normalized_path.ends_with("/amcache.hve") {
        HiveKind::Amcache
    } else if normalized_path.ends_with("/security") {
        HiveKind::Security
    } else {
        HiveKind::Other
    }
}
