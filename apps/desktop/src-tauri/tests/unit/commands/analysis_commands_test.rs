use app_services::analysis_service;
use transport::commands::ClassifyFilesRequest;

use super::support::resolve_sample_size;

#[test]
fn sample_size_defaults_and_validates_bounds() {
    assert_eq!(
        resolve_sample_size(&ClassifyFilesRequest {
            data_source_id: "ds-test".to_string(),
            sample_size: None,
        })
        .unwrap(),
        analysis_service::DEFAULT_SAMPLE_SIZE
    );
    assert_eq!(
        resolve_sample_size(&ClassifyFilesRequest {
            data_source_id: "ds-test".to_string(),
            sample_size: Some(1)
        })
        .unwrap(),
        1
    );
    assert!(resolve_sample_size(&ClassifyFilesRequest {
        data_source_id: "ds-test".to_string(),
        sample_size: Some(0)
    })
    .is_err());
    assert!(resolve_sample_size(&ClassifyFilesRequest {
        data_source_id: "ds-test".to_string(),
        sample_size: Some(analysis_service::MAX_SAMPLE_SIZE + 1)
    })
    .is_err());
}
