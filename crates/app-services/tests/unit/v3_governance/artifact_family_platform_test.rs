use super::*;

#[test]
fn all_production_linux_families_are_classified_as_linux() {
    for family in [
        "LinuxJournal",
        "LinuxWtmp",
        "LinuxBashCommand",
        "LinuxAptEvent",
        "LinuxCronJob",
        "LinuxSudoEvent",
        "LinuxSystemConfig",
        "LinuxWebSite",
        "LinuxWebAccessLog",
        "LinuxWebErrorLog",
        "LinuxWebFinding",
        "LinuxMysqlConfig",
        "LinuxMysqlLogEntry",
        "LinuxMysqlFinding",
    ] {
        assert_eq!(
            classify_artifact_family(family),
            ArtifactFamilyPlatform::Linux,
            "{family}"
        );
    }
}

#[test]
fn all_current_windows_capability_families_are_classified_as_windows() {
    for family in [
        "LNK",
        "Prefetch",
        "RegistryValue",
        "EvtxSecurityEvent",
        "BrowserHistory",
        "BrowserPassword",
        "EmailMessage",
    ] {
        assert_eq!(
            classify_artifact_family(family),
            ArtifactFamilyPlatform::Windows,
            "{family}"
        );
    }
}

#[test]
fn unknown_family_is_not_silently_classified_as_windows() {
    assert_eq!(
        classify_artifact_family("UnknownFamily"),
        ArtifactFamilyPlatform::Unknown
    );
    assert_eq!(
        classify_artifact_family("CustomArtifact"),
        ArtifactFamilyPlatform::Unknown
    );
}
