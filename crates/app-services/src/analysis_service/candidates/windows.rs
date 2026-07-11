use super::common::{EvidenceCategoryDef, EvidencePathPattern};

pub(super) const SYSTEM_INFORMATION_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "SystemInformation",
    display_name: "System information",
    evidence_kind: "registry_hive",
    parser: "registry.system_info",
    artifact_families: &["Registry"],
    patterns: &[
        EvidencePathPattern::Suffix("/windows/system32/config/system"),
        EvidencePathPattern::Suffix("/windows/system32/config/software"),
        EvidencePathPattern::Suffix("/windows/system32/config/sam"),
        EvidencePathPattern::Suffix("/windows/system32/config/security"),
        EvidencePathPattern::Suffix("/ntuser.dat"),
        EvidencePathPattern::Suffix("/usrclass.dat"),
    ],
    matcher: None,
};

pub(super) const REGISTRY_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "Registry",
    display_name: "注册表",
    evidence_kind: "registry_hive",
    parser: "registry.hive",
    artifact_families: &["Registry"],
    patterns: &[
        EvidencePathPattern::Suffix("/windows/system32/config/system"),
        EvidencePathPattern::Suffix("/windows/system32/config/software"),
        EvidencePathPattern::Suffix("/windows/system32/config/sam"),
        EvidencePathPattern::Suffix("/windows/system32/config/security"),
        EvidencePathPattern::Suffix("/windows/system32/config/default"),
        EvidencePathPattern::Suffix("/ntuser.dat"),
        EvidencePathPattern::Suffix("/usrclass.dat"),
        EvidencePathPattern::Suffix("/windows/appcompat/programs/amcache.hve"),
    ],
    matcher: None,
};

pub(super) const EVENT_LOGS_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "EventLogs",
    display_name: "Event logs",
    evidence_kind: "event_log",
    parser: "evtx.boot_shutdown",
    artifact_families: &[],
    patterns: &[EvidencePathPattern::Suffix(".evtx")],
    matcher: None,
};

pub(super) const PROGRAM_EXECUTION_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "ProgramExecution",
    display_name: "Program execution",
    evidence_kind: "execution_artifact",
    parser: "prefetch.amcache.shimcache",
    artifact_families: &[
        "Prefetch",
        "RegistryAmcacheApplication",
        "RegistryAmcacheApplicationFile",
    ],
    patterns: &[
        EvidencePathPattern::Suffix(".pf"),
        EvidencePathPattern::Suffix("/windows/appcompat/programs/amcache.hve"),
        EvidencePathPattern::Contains("/windows/prefetch/"),
    ],
    matcher: None,
};

pub(super) const USER_ACTIVITY_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "UserActivity",
    display_name: "User activity",
    evidence_kind: "user_activity",
    parser: "lnk.jumplist.shellbags",
    artifact_families: &["LNK", "JumpList"],
    patterns: &[
        EvidencePathPattern::Suffix(".lnk"),
        EvidencePathPattern::Suffix(".automaticdestinations-ms"),
        EvidencePathPattern::Suffix(".customdestinations-ms"),
        EvidencePathPattern::Contains("/recent/"),
        EvidencePathPattern::Contains("/shellbags"),
    ],
    matcher: None,
};

pub(super) const RECYCLE_BIN_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "RecycleBin",
    display_name: "Recycle bin",
    evidence_kind: "recycle_bin",
    parser: "recycle_bin",
    artifact_families: &["RecycleBin"],
    patterns: &[
        EvidencePathPattern::Contains("/$recycle.bin/"),
        EvidencePathPattern::Contains("/recycler/"),
    ],
    matcher: None,
};

pub(super) const THUMBNAILS_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "Thumbnails",
    display_name: "Thumbnail cache",
    evidence_kind: "thumbnail_cache",
    parser: "thumbcache",
    artifact_families: &["Thumbcache"],
    patterns: &[
        EvidencePathPattern::Contains("/thumbcache_"),
        EvidencePathPattern::Contains("/iconcache_"),
    ],
    matcher: None,
};

pub(super) const RESOURCE_USAGE_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "ResourceUsage",
    display_name: "Resource usage",
    evidence_kind: "resource_usage",
    parser: "sru",
    artifact_families: &["SRU"],
    patterns: &[EvidencePathPattern::Suffix(
        "/windows/system32/sru/srudb.dat",
    )],
    matcher: None,
};

pub(super) const BROWSER_HISTORY_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "BrowserHistory",
    display_name: "浏览器历史",
    evidence_kind: "browser_history",
    parser: "browser.history",
    artifact_families: &[
        "BrowserHistory",
        "BrowserDownload",
        "BrowserCookie",
        "BrowserSessionTab",
        "BrowserPassword",
    ],
    patterns: &[
        EvidencePathPattern::Contains("/google/chrome/user data/"),
        EvidencePathPattern::Contains("/microsoft/edge/user data/"),
        EvidencePathPattern::Contains("/mozilla/firefox/profiles/"),
        EvidencePathPattern::Suffix("/history"),
        EvidencePathPattern::Suffix("/archived history"),
        EvidencePathPattern::Suffix("/cookies"),
        EvidencePathPattern::Suffix("/cookies.sqlite"),
        EvidencePathPattern::Suffix("/login data"),
        EvidencePathPattern::Suffix("/logins.json"),
        EvidencePathPattern::Suffix("/last session"),
        EvidencePathPattern::Suffix("/last tabs"),
        EvidencePathPattern::Suffix("/current session"),
        EvidencePathPattern::Suffix("/current tabs"),
        EvidencePathPattern::Suffix("/recovery.jsonlz4"),
        EvidencePathPattern::Suffix("/previous.jsonlz4"),
        EvidencePathPattern::Suffix("/sessionstore.jsonlz4"),
        EvidencePathPattern::Suffix("/places.sqlite"),
    ],
    matcher: Some(is_browser_history_path),
};

pub(crate) fn is_browser_history_path(normalized: &str) -> bool {
    if normalized.contains("/google/chrome/user data/")
        || normalized.contains("/microsoft/edge/user data/")
    {
        return normalized.ends_with("/history")
            || normalized.ends_with("/archived history")
            || normalized.ends_with("/cookies")
            || normalized.ends_with("/login data")
            || normalized.ends_with("/last session")
            || normalized.ends_with("/last tabs")
            || normalized.ends_with("/current session")
            || normalized.ends_with("/current tabs");
    }
    if normalized.contains("/mozilla/firefox/profiles/") {
        return normalized.ends_with("/places.sqlite")
            || normalized.ends_with("/cookies.sqlite")
            || normalized.ends_with("/logins.json")
            || normalized.ends_with("/recovery.jsonlz4")
            || normalized.ends_with("/previous.jsonlz4")
            || normalized.ends_with("/sessionstore.jsonlz4");
    }
    false
}
