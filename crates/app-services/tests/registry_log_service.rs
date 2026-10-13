#[path = "support/registry_browser_case.rs"]
mod fixture;
#[path = "support/registry_browser_hive.rs"]
mod hive_fixture;
#[path = "support/registry_logs.rs"]
mod logs;
use app_services::registry_browser_service::*;
use transport::dto::*;

#[test]
fn recovered_sessions_bind_exact_source_logs_and_keep_base_bytes_separate() {
    let case = fixture::BrowserCase::new();
    let registry = RegistryBrowserRegistry::default();
    let evidence = case.temp.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let mut base = hive_fixture::hive_bytes();
    logs::seal(&mut base, 11, 10);
    let page = |number: u32| {
        let mut page = base[4096..4608].to_vec();
        page[140..144].copy_from_slice(&number.to_le_bytes());
        page
    };
    let a = logs::log(&base, 11, &[(0, page(3))]);
    let b = logs::log(&base, 10, &[(0, page(2))]);
    for (name, bytes) in [
        ("SOFTWARE", &base),
        ("SOFTWARE.LOG1", &a),
        ("SOFTWARE.LOG2", &b),
    ] {
        std::fs::write(evidence.join(name), bytes).unwrap();
    }
    case.active
        .with_conn(|conn| {
            let source = case.source(
                conn,
                "source-a",
                &evidence,
                domain::DataSourceKind::LogicalDirectory,
            );
            for (id, path, size) in [
                ("hive", "SOFTWARE", base.len()),
                ("log1", "SOFTWARE.LOG1", a.len()),
                ("log2", "SOFTWARE.LOG2", b.len()),
                ("unrelated", "Other/SOFTWARE.LOG1", a.len()),
            ] {
                fixture::entry(&source, "source-a", id, path, size as u64);
            }
            let foreign = case.source(
                conn,
                "source-b",
                &evidence,
                domain::DataSourceKind::LogicalDirectory,
            );
            fixture::entry(
                &foreign,
                "source-b",
                "wrong-log",
                "SOFTWARE.LOG1",
                a.len() as u64,
            );
            let ctx = case.context(conn);
            let base_session = open_hive(
                &registry,
                &ctx,
                RegistryHiveOpenRequestDto {
                    file_id: "ds:source-a:hive".into(),
                    view: RegistryHiveViewDto::Base,
                },
            )
            .unwrap();
            let recovered = open_hive(
                &registry,
                &ctx,
                RegistryHiveOpenRequestDto {
                    file_id: "ds:source-a:hive".into(),
                    view: RegistryHiveViewDto::Recovered,
                },
            )
            .unwrap();
            let report = recovered.recovery.as_ref().unwrap();
            assert_eq!(report.status, RegistryRecoveryStatusDto::Recovered);
            assert_eq!(report.applied_entries, 2);
            assert_ne!(report.base_sha256, report.result_sha256);
            assert_eq!(report.logs[0].file_id.as_deref(), Some("ds:source-a:log1"));
            assert_eq!(report.logs[1].file_id.as_deref(), Some("ds:source-a:log2"));
            assert_ne!(base_session.root_key.key_id, recovered.root_key.key_id);
            for (session, expected) in [(&base_session, "42"), (&recovered, "3")] {
                for _ in 0..3 {
                    let page = list_values(
                        &registry,
                        &ctx,
                        RegistryValuesRequestDto {
                            session_id: session.session_id.clone(),
                            key_id: session.root_key.key_id.clone(),
                            limit: 10,
                            cursor: None,
                        },
                    )
                    .unwrap();
                    let range = read_value_range(
                        &registry,
                        &ctx,
                        RegistryValueRangeRequestDto {
                            session_id: session.session_id.clone(),
                            key_id: session.root_key.key_id.clone(),
                            value_id: page.values[0].value_id.clone(),
                            offset: 0,
                            length: 65536,
                        },
                    )
                    .unwrap();
                    assert_eq!(range.decoded_text.as_deref(), Some(expected));
                }
            }
            assert_eq!(registry.stats().unwrap().hive_reads, 2);
            assert_eq!(registry.stats().unwrap().log_reads, 2);
            assert!(list_values(
                &registry,
                &ctx,
                RegistryValuesRequestDto {
                    session_id: recovered.session_id.clone(),
                    key_id: base_session.root_key.key_id.clone(),
                    limit: 10,
                    cursor: None
                }
            )
            .is_err());
            registry.invalidate_case(&case.active.meta.id.0).unwrap();
            assert_eq!(registry.stats().unwrap().retained_bytes, 0);
            Ok(())
        })
        .unwrap();
    assert_eq!(std::fs::read(evidence.join("SOFTWARE")).unwrap(), base);
    assert_eq!(std::fs::read(evidence.join("SOFTWARE.LOG1")).unwrap(), a);
}

#[test]
fn dirty_hive_without_logs_fails_and_clean_hive_keeps_original_snapshot() {
    let case = fixture::BrowserCase::new();
    let registry = RegistryBrowserRegistry::default();
    let evidence = case.temp.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let mut dirty = hive_fixture::hive_bytes();
    logs::seal(&mut dirty, 2, 1);
    std::fs::write(evidence.join("SOFTWARE"), &dirty).unwrap();
    case.active
        .with_conn(|conn| {
            let source = case.source(
                conn,
                "source",
                &evidence,
                domain::DataSourceKind::LogicalDirectory,
            );
            fixture::entry(&source, "source", "hive", "SOFTWARE", dirty.len() as u64);
            let ctx = case.context(conn);
            let request = || RegistryHiveOpenRequestDto {
                file_id: "ds:source:hive".into(),
                view: RegistryHiveViewDto::Recovered,
            };
            let error = open_hive(&registry, &ctx, request()).unwrap_err();
            assert!(matches!(error, RegistryBrowserError::Recovery(_)));
            assert_eq!(registry.stats().unwrap().session_count, 0);
            logs::seal(&mut dirty, 2, 2);
            std::fs::write(evidence.join("SOFTWARE"), &dirty).unwrap();
            let result = open_hive(&registry, &ctx, request()).unwrap();
            let report = result.recovery.unwrap();
            assert_eq!(report.status, RegistryRecoveryStatusDto::NotNeeded);
            assert_eq!(report.base_sha256, report.result_sha256);
            Ok(())
        })
        .unwrap();
}
