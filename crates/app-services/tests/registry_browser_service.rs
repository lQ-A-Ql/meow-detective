#[path = "support/registry_browser_case.rs"]
mod fixture;
#[path = "support/registry_browser_hive.rs"]
mod hive_fixture;
use app_services::registry_browser_service::*;
use transport::dto::*;

#[test]
fn source_bound_sessions_read_once_and_reject_foreign_ids_cursors_and_generations() {
    let case = fixture::BrowserCase::new();
    let registry = RegistryBrowserRegistry::default();
    let evidence = case.temp.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let bytes = hive_fixture::hive_bytes();
    std::fs::write(evidence.join("SOFTWARE"), &bytes).unwrap();
    std::fs::write(evidence.join("NTUSER.DAT"), &bytes).unwrap();
    case.active
        .with_conn(|conn| {
            let source = case.source(
                conn,
                "source-a",
                &evidence,
                domain::DataSourceKind::LogicalDirectory,
            );
            fixture::entry(
                &source,
                "source-a",
                "hive-1",
                "SOFTWARE",
                bytes.len() as u64,
            );
            fixture::entry(
                &source,
                "source-a",
                "hive-2",
                "NTUSER.DAT",
                bytes.len() as u64,
            );
            let foreign = case.source(
                conn,
                "source-b",
                &evidence,
                domain::DataSourceKind::LogicalDirectory,
            );
            fixture::entry(
                &foreign,
                "source-b",
                "hive-1",
                "SOFTWARE",
                bytes.len() as u64,
            );
            let ctx = case.context(conn);
            let page = list_hives(
                &registry,
                &ctx,
                RegistryHiveListRequestDto {
                    data_source_id: "source-a".into(),
                    limit: 1,
                    cursor: None,
                },
            )
            .unwrap();
            assert_eq!(page.coverage.declared_count, 2);
            assert!(page.next_cursor.is_some());
            assert!(list_hives(
                &registry,
                &ctx,
                RegistryHiveListRequestDto {
                    data_source_id: "source-b".into(),
                    limit: 1,
                    cursor: page.next_cursor.clone()
                }
            )
            .is_err());
            let next = list_hives(
                &registry,
                &ctx,
                RegistryHiveListRequestDto {
                    data_source_id: "source-a".into(),
                    limit: 1,
                    cursor: page.next_cursor,
                },
            )
            .unwrap();
            assert_eq!(next.hives.len(), 1);
            let session = open_hive(
                &registry,
                &ctx,
                RegistryHiveOpenRequestDto {
                    file_id: page.hives[0].file_id.clone(),
                    view: RegistryHiveViewDto::Base,
                },
            )
            .unwrap();
            assert_eq!(session.source.source_revision, page.source_revision);
            let hardware = read_hardware(
                &registry,
                &ctx,
                RegistryHardwareRequestDto {
                    session_id: session.session_id.clone(),
                },
            )
            .unwrap();
            assert_eq!(hardware.source, session.source);
            assert!(hardware.devices.is_empty());
            for _ in 0..3 {
                let key = resolve_key(
                    &registry,
                    &ctx,
                    RegistryKeyResolveRequestDto {
                        session_id: session.session_id.clone(),
                        key_path: "".into(),
                    },
                )
                .unwrap();
                let values = list_values(
                    &registry,
                    &ctx,
                    RegistryValuesRequestDto {
                        session_id: session.session_id.clone(),
                        key_id: key.key_id.clone(),
                        limit: 10,
                        cursor: None,
                    },
                )
                .unwrap();
                assert_eq!(values.values.len(), 1);
                assert_eq!(values.values[0].name, "");
                let range = read_value_range(
                    &registry,
                    &ctx,
                    RegistryValueRangeRequestDto {
                        session_id: session.session_id.clone(),
                        key_id: key.key_id,
                        value_id: values.values[0].value_id.clone(),
                        offset: 0,
                        length: 65536,
                    },
                )
                .unwrap();
                assert_eq!(range.bytes, 42u32.to_le_bytes());
                assert_eq!(range.decoded_text.as_deref(), Some("42"));
            }
            assert_eq!(registry.stats().unwrap().hive_reads, 1);
            let foreign_case_id = domain::CaseId("different-case".into());
            let foreign_ctx = RegistryBrowserContext {
                case_id: &foreign_case_id,
                ..case.context(conn)
            };
            assert!(matches!(
                read_hardware(
                    &registry,
                    &foreign_ctx,
                    RegistryHardwareRequestDto {
                        session_id: session.session_id.clone()
                    }
                ),
                Err(RegistryBrowserError::ExpiredSession)
            ));
            assert!(matches!(
                resolve_key(
                    &registry,
                    &foreign_ctx,
                    RegistryKeyResolveRequestDto {
                        session_id: session.session_id.clone(),
                        key_path: "".into()
                    }
                ),
                Err(RegistryBrowserError::ExpiredSession)
            ));
            let invalid = RegistryValuesRequestDto {
                session_id: session.session_id.clone(),
                key_id: "arbitrary-cell-offset".into(),
                limit: 10,
                cursor: None,
            };
            assert!(list_values(&registry, &ctx, invalid).is_err());
            case.preview
                .invalidate_source(&case.active.meta.id.0, "source-a")
                .unwrap();
            assert!(matches!(
                read_hardware(
                    &registry,
                    &ctx,
                    RegistryHardwareRequestDto {
                        session_id: session.session_id.clone()
                    }
                ),
                Err(RegistryBrowserError::ExpiredSession)
            ));
            assert!(matches!(
                resolve_key(
                    &registry,
                    &ctx,
                    RegistryKeyResolveRequestDto {
                        session_id: session.session_id.clone(),
                        key_path: "".into()
                    }
                ),
                Err(RegistryBrowserError::ExpiredSession)
            ));
            close_hive(
                &registry,
                &case.active.meta.id.0,
                RegistryHiveCloseRequestDto {
                    session_id: session.session_id,
                },
            )
            .unwrap();
            assert_eq!(registry.stats().unwrap().session_count, 0);
            for _ in 0..9 {
                open_hive(
                    &registry,
                    &ctx,
                    RegistryHiveOpenRequestDto {
                        file_id: page.hives[0].file_id.clone(),
                        view: RegistryHiveViewDto::Base,
                    },
                )
                .unwrap();
            }
            assert_eq!(registry.stats().unwrap().session_count, 8);
            registry
                .invalidate_source(&case.active.meta.id.0, "source-a")
                .unwrap();
            assert_eq!(registry.stats().unwrap().retained_bytes, 0);
            registry.invalidate_case(&case.active.meta.id.0).unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(std::fs::read(evidence.join("SOFTWARE")).unwrap(), bytes);
}
