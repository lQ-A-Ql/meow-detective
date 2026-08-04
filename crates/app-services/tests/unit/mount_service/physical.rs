use domain::DataSourceKind;

use super::{prepared_kind, PreparedPhysicalImageKind};

#[test]
fn physical_mount_kind_accepts_only_e01_and_raw_images() {
    assert_eq!(
        prepared_kind(&DataSourceKind::E01).unwrap(),
        PreparedPhysicalImageKind::E01
    );
    assert_eq!(
        prepared_kind(&DataSourceKind::Raw).unwrap(),
        PreparedPhysicalImageKind::Raw
    );
    assert!(prepared_kind(&DataSourceKind::LogicalDirectory).is_err());
}
