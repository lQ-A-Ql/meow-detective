use super::*;

fn install() -> EmulationInstallDto {
    super::super::emulation_linux::linux_install_skeleton(2)
}

#[test]
fn xfs_assessment_annotations_are_fail_closed_and_deduplicated() {
    let mut installs = vec![install()];
    annotate_xfs_assessments(
        &mut installs,
        &[
            XfsLogAssessment::Clean,
            XfsLogAssessment::Dirty,
            XfsLogAssessment::Unverified,
            XfsLogAssessment::Unverified,
        ],
    );

    assert_eq!(
        installs[0].boot_risk_notes,
        vec!["xfs-log-dirty", "xfs-log-unverified"]
    );
}

#[test]
fn clean_or_absent_xfs_assessments_add_no_risk() {
    for assessments in [Vec::new(), vec![XfsLogAssessment::Clean]] {
        let mut installs = vec![install()];
        annotate_xfs_assessments(&mut installs, &assessments);
        assert!(installs[0].boot_risk_notes.is_empty());
    }
}
