use domain::DataSourceId;
use persistence_sqlite::repositories::android_analysis_repo::{
    AndroidAnalysisRepo, AndroidFactRecord, AndroidPackageRecord,
};
use persistence_sqlite::{open_in_memory, runner};

#[test]
fn android_analysis_repo_replaces_and_pages_source_records() {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    let source = DataSourceId("source-android".to_string());
    let repository = AndroidAnalysisRepo::new(&connection);

    repository
        .replace_for_source(
            &source,
            &[AndroidFactRecord {
                field: "model".to_string(),
                value: "Pixel".to_string(),
                confidence: "high".to_string(),
                source_file_id: "file:build-prop".to_string(),
                source_path: "/system/build.prop".to_string(),
                source_rank: 1,
                parser: "android.system-packages.v1".to_string(),
                warning: None,
            }],
            &[
                AndroidPackageRecord {
                    package_name: "com.example.zeta".to_string(),
                    app_name: None,
                    version_code: Some("2".to_string()),
                    install_time: None,
                    update_time: None,
                    installer: None,
                    uid: Some(10001),
                    code_path: Some("/data/app/zeta".to_string()),
                    source_file_id: "file:packages".to_string(),
                    source_path: "/data/system/packages.xml".to_string(),
                    parser: "android.system-packages.v1".to_string(),
                    warning: None,
                },
                AndroidPackageRecord {
                    package_name: "com.example.alpha".to_string(),
                    app_name: None,
                    version_code: Some("1".to_string()),
                    install_time: None,
                    update_time: None,
                    installer: None,
                    uid: Some(10002),
                    code_path: Some("/data/app/alpha".to_string()),
                    source_file_id: "file:packages".to_string(),
                    source_path: "/data/system/packages.xml".to_string(),
                    parser: "android.system-packages.v1".to_string(),
                    warning: None,
                },
            ],
        )
        .expect("replace Android analysis records");

    let facts = repository.list_facts(&source).expect("list facts");
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].field, "model");
    assert_eq!(
        repository.count_packages(&source).expect("count packages"),
        2
    );

    let page = repository
        .list_packages_page(&source, 0, 1)
        .expect("list package page");
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].package_name, "com.example.alpha");

    repository
        .replace_for_source(&source, &[], &[])
        .expect("replace with empty result");
    assert!(repository
        .list_facts(&source)
        .expect("list facts")
        .is_empty());
    assert_eq!(
        repository.count_packages(&source).expect("count packages"),
        0
    );
}
