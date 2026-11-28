use domain::{
    structure::{
        CancellationScope, CapabilityKind, ConflictReason, EvidenceRequirement, MetadataFailure,
        ResourceKind, SourceRevisionRef, TargetFailure,
    },
    CaseId, DataSourceId,
};
use evidence_access::file_service::{FileServiceError, SourceReadContext};
use persistence_sqlite::{
    repositories::{datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo},
    DbError,
};
use serde_json::{json, Value};

use crate::{
    cluster_service::{self, ClusterServiceError, KubernetesParsedArtifact, KubernetesParserError},
    structure_runtime::{ExecutionContext, RuntimeError},
};

pub(super) fn analyze(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<Value, RuntimeError> {
    validate_source(context, source)?;
    check_active(context)?;
    let source_id = DataSourceId(source.source_id.clone());
    let case_id = CaseId(source.case_id.clone());
    let storage = DataSourceRepo::new(context.connection)
        .find_storage(&source_id)
        .map_err(database_error)?
        .ok_or_else(missing_namespace)?;
    if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
        return Err(missing_namespace());
    }
    let connection = source_kernel::source_db::open_registered_source_db_read_only(
        context.connection,
        context.case_root,
        &source_id,
    )
    .map_err(database_error)?;
    let inventory = cluster_service::discover_kubernetes_member_artifacts(&connection, &source_id)
        .map_err(cluster_error)?;
    inventory.require_complete().map_err(parser_error)?;
    if inventory.artifacts.is_empty() {
        return Err(RuntimeError::Target(TargetFailure::MissingCapabilities {
            capabilities: [CapabilityKind::ResourceRecords].into_iter().collect(),
        }));
    }
    check_active(context)?;
    let mut reader = SourceReadContext::new(
        &connection,
        context.connection,
        context.case_root,
        &case_id,
        &source_id,
    );
    let artifacts = parse_artifacts(context, &source_id, &mut reader, &inventory.artifacts)?;
    validate_source(context, source)?;
    check_active(context)?;
    let output = json!({
        "analysis": "kubernetes",
        "sourceRevision": source,
        "controlPlaneObserved": inventory.control_plane_observed,
        "inventory": {
            "scannedEntries": inventory.scanned_entries,
            "matchedEntries": inventory.matched_entries,
            "artifactCount": artifacts.len(),
            "complete": true,
        },
        "artifacts": artifacts,
        "diagnostics": [],
    });
    check_bytes(
        serialized_bytes(&output)?,
        context.request.resource_policy.page_bytes,
    )?;
    Ok(output)
}

fn parse_artifacts(
    context: &ExecutionContext<'_>,
    source_id: &DataSourceId,
    reader: &mut SourceReadContext<'_>,
    artifacts: &[cluster_service::KubernetesMemberArtifact],
) -> Result<Vec<Value>, RuntimeError> {
    let mut output = Vec::with_capacity(artifacts.len());
    let mut output_bytes = 0u64;
    for artifact in artifacts {
        if artifact.data_source_id != *source_id {
            return Err(RuntimeError::InvalidOutput);
        }
        let value = parse_artifact(context, reader, artifact)?;
        output_bytes = output_bytes
            .checked_add(serialized_bytes(&value)?)
            .ok_or(RuntimeError::Capacity)?;
        check_bytes(output_bytes, context.request.resource_policy.page_bytes)?;
        output.push(value);
    }
    Ok(output)
}

fn parse_artifact(
    context: &ExecutionContext<'_>,
    reader: &mut SourceReadContext<'_>,
    artifact: &cluster_service::KubernetesMemberArtifact,
) -> Result<Value, RuntimeError> {
    check_active(context)?;
    check_bytes(artifact.size, context.request.resource_policy.bytes)?;
    let (parsed, digest) =
        cluster_service::read_and_parse(reader, context.case_root, artifact, context.cancel)
            .map_err(cluster_error)?;
    let (status, evidence, diagnostics) = match parsed {
        Some(parsed) => ("parsed", evidence_json(parsed)?, Vec::<&str>::new()),
        None => ("candidate", Value::Null, vec!["parser_unavailable"]),
    };
    Ok(json!({
        "sourceId": artifact.data_source_id.0,
        "fileId": artifact.file_id.0,
        "path": artifact.path,
        "kind": artifact.kind.as_str(),
        "size": artifact.size,
        "deleted": artifact.deleted,
        "encrypted": artifact.encrypted,
        "sha256": digest,
        "status": status,
        "evidenceJson": evidence,
        "diagnostics": diagnostics,
    }))
}

fn validate_source(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<(), RuntimeError> {
    let capture = context
        .request
        .source_scope
        .iter()
        .find(|capture| capture.source == *source)
        .filter(|_| source.case_id == context.request.case_id)
        .ok_or(RuntimeError::InvalidDefinition)?;
    let (current, content) = SourceRevisionRepo::new(context.connection)
        .current(&source.case_id, &source.source_id)
        .map_err(database_error)?;
    if current != *source || content.acquisition_id != capture.acquisition_id {
        return Err(RuntimeError::Target(TargetFailure::Conflicted {
            reason: ConflictReason::InputChanged,
        }));
    }
    Ok(())
}

fn check_active(context: &ExecutionContext<'_>) -> Result<(), RuntimeError> {
    source_kernel::source_runtime::range_view::check_active(
        context.cancel,
        context.request.resource_policy.deadline_unix_ms,
    )
    .map_err(super::super::proof_state::classify)
}

fn check_bytes(required: u64, limit: u64) -> Result<(), RuntimeError> {
    if required > limit {
        return Err(RuntimeError::Target(TargetFailure::ResourceLimit {
            resource: ResourceKind::Bytes,
            required,
            limit,
        }));
    }
    Ok(())
}

fn evidence_json(parsed: KubernetesParsedArtifact) -> Result<Value, RuntimeError> {
    Ok(match parsed {
        KubernetesParsedArtifact::Kubeconfig(summary) => kubeconfig_json(summary),
        KubernetesParsedArtifact::StaticPodManifest(manifests) => json!({
            "manifests": manifests.into_iter().map(|manifest| json!({
                "apiVersion": manifest.api_version,
                "kind": manifest.kind,
                "name": manifest.name,
                "namespace": manifest.namespace,
                "containerCount": manifest.container_count,
                "containers": manifest.containers.into_iter().map(|container| json!({
                    "name": container.name,
                    "image": container.image,
                    "privileged": container.privileged,
                    "allowPrivilegeEscalation": container.allow_privilege_escalation,
                    "commandCount": container.command_count,
                    "argsCount": container.args_count,
                })).collect::<Vec<_>>(),
                "hostNetwork": manifest.host_network,
                "hostPid": manifest.host_pid,
                "hostIpc": manifest.host_ipc,
                "hostPathCount": manifest.host_path_count,
                "privilegedContainerCount": manifest.privileged_container_count,
            })).collect::<Vec<_>>(),
        }),
        KubernetesParsedArtifact::AuditLog(summary) => audit_json(summary)?,
        KubernetesParsedArtifact::EtcdBackend(summary) => {
            if !summary.checksum_verified {
                return Err(corrupt(MetadataFailure::InvalidChecksum));
            }
            json!({
                "pageSize": summary.page_size,
                "txid": summary.txid,
                "rootPage": summary.root_page,
                "bucketCount": summary.bucket_count,
                "keyCount": summary.key_count,
                "checksumVerified": summary.checksum_verified,
                "entries": summary.entries.into_iter().map(|entry| json!({
                    "bucketPath": entry.bucket_path,
                    "key": entry.key,
                    "valueLength": entry.value_length,
                    "nestedBucket": entry.nested_bucket,
                    "valueRedacted": entry.value_redacted,
                })).collect::<Vec<_>>(),
            })
        }
        KubernetesParsedArtifact::EtcdWal(summary) => {
            if summary.checksum_failures != 0 {
                return Err(corrupt(MetadataFailure::InvalidChecksum));
            }
            if summary.truncated_tail {
                return Err(corrupt(MetadataFailure::Truncated));
            }
            json!({
                "metadataCount": summary.metadata_count,
                "entryCount": summary.entry_count,
                "stateCount": summary.state_count,
                "checksumFailures": summary.checksum_failures,
                "truncatedTail": summary.truncated_tail,
                "records": summary.records.into_iter().map(|record| json!({
                    "offset": record.offset,
                    "recordType": record.record_type,
                    "payloadLength": record.payload_length,
                    "checksumValid": record.checksum_valid,
                    "protobufFieldCount": record.protobuf_field_count,
                })).collect::<Vec<_>>(),
            })
        }
        KubernetesParsedArtifact::CniNetwork(summary) => json!({
            "name": summary.name,
            "cniVersion": summary.cni_version,
            "pluginTypes": summary.plugin_types,
            "ipamType": summary.ipam_type,
            "subnets": summary.subnets,
            "routes": summary.routes,
        }),
    })
}

fn kubeconfig_json(summary: cluster_service::KubeconfigSummary) -> Value {
    json!({
        "apiVersion": summary.api_version,
        "currentContext": summary.current_context,
        "clusters": summary.clusters.into_iter().map(|cluster| json!({
            "name": cluster.name,
            "server": cluster.server,
            "certificateAuthorityPath": cluster.certificate_authority_path,
            "certificateAuthorityDataPresent": cluster.certificate_authority_data_present,
            "insecureSkipTlsVerify": cluster.insecure_skip_tls_verify,
        })).collect::<Vec<_>>(),
        "contexts": summary.contexts.into_iter().map(|context| json!({
            "name": context.name,
            "cluster": context.cluster,
            "user": context.user,
            "namespace": context.namespace,
        })).collect::<Vec<_>>(),
        "users": summary.users.into_iter().map(|user| json!({
            "name": user.name,
            "clientCertificatePath": user.client_certificate_path,
            "clientKeyPath": user.client_key_path,
            "clientCertificateDataPresent": user.client_certificate_data_present,
            "clientKeyDataPresent": user.client_key_data_present,
            "tokenPresent": user.token_present,
            "authProvider": user.auth_provider,
        })).collect::<Vec<_>>(),
    })
}

fn audit_json(summary: cluster_service::KubernetesAuditParseResult) -> Result<Value, RuntimeError> {
    if summary.invalid_lines != 0 || summary.truncated {
        return Err(corrupt(if summary.truncated {
            MetadataFailure::Truncated
        } else {
            MetadataFailure::InvalidStructure
        }));
    }
    Ok(json!({
        "invalidLines": summary.invalid_lines,
        "truncated": summary.truncated,
        "events": summary.events.into_iter().map(|event| json!({
            "lineNumber": event.line_number,
            "timestamp": event.timestamp,
            "stage": event.stage,
            "verb": event.verb,
            "username": event.username,
            "resource": event.resource,
            "subresource": event.subresource,
            "namespace": event.namespace,
            "name": event.name,
            "responseCode": event.response_code,
            "sourceIp": event.source_ip,
            "indicators": event.indicators.into_iter().map(|indicator| {
                use cluster_service::KubernetesAuditIndicator;
                match indicator {
                    KubernetesAuditIndicator::InteractiveExec => "interactive_exec",
                    KubernetesAuditIndicator::SecretAccess => "secret_access",
                    KubernetesAuditIndicator::RbacChange => "rbac_change",
                    KubernetesAuditIndicator::PrivilegedPod => "privileged_pod",
                    KubernetesAuditIndicator::AnonymousAccess => "anonymous_access",
                }
            }).collect::<Vec<_>>(),
            "auditId": event.audit_id,
            "stageTimestamp": event.stage_timestamp,
            "resourceObject": event.resource_object,
            "objectUid": event.object_uid,
        })).collect::<Vec<_>>(),
    }))
}

fn cluster_error(error: ClusterServiceError) -> RuntimeError {
    match error {
        ClusterServiceError::KubernetesParser(error) => parser_error(error),
        ClusterServiceError::FileRead(error) => file_error(error),
        ClusterServiceError::Db(error) => database_error(error),
        ClusterServiceError::Io(error) => super::super::proof_state::classify(error),
        ClusterServiceError::Cancelled => cancelled(),
        ClusterServiceError::Unsupported => unsupported(),
        _ => RuntimeError::InvalidOutput,
    }
}

fn parser_error(error: KubernetesParserError) -> RuntimeError {
    match error {
        KubernetesParserError::Limit { kind, actual, max } => {
            let resource = match kind {
                "artifact bytes" | "audit bytes" | "audit line bytes" | "yaml bytes"
                | "etcd WAL bytes" | "etcd WAL payload" => ResourceKind::Bytes,
                _ => ResourceKind::ActiveWork,
            };
            RuntimeError::Target(TargetFailure::ResourceLimit {
                resource,
                required: actual as u64,
                limit: max as u64,
            })
        }
        KubernetesParserError::Truncated { .. } => corrupt(MetadataFailure::Truncated),
        KubernetesParserError::Bolt(error) => {
            use evidence_access::bolt_reader_service::BoltReadError;
            match error {
                BoltReadError::Cancelled => cancelled(),
                BoltReadError::Timeout => RuntimeError::Target(TargetFailure::DeadlineExceeded),
                BoltReadError::Io(error) => super::super::proof_state::classify(error),
                BoltReadError::Decode => corrupt(MetadataFailure::InvalidStructure),
                BoltReadError::InputLimit | BoltReadError::OutputLimit => unsupported(),
                BoltReadError::Budget(source_kernel::resource_budget::ResourceError::Cancelled) => {
                    cancelled()
                }
                BoltReadError::InvalidResponse | BoltReadError::Protocol(_) => {
                    RuntimeError::InvalidOutput
                }
                _ => RuntimeError::Target(TargetFailure::IoUnavailable),
            }
        }
        KubernetesParserError::WorkspaceRequired => RuntimeError::InvalidDefinition,
        _ => corrupt(MetadataFailure::InvalidStructure),
    }
}

fn file_error(error: FileServiceError) -> RuntimeError {
    match error {
        FileServiceError::Db(error) => database_error(error),
        FileServiceError::Io(error) => super::super::proof_state::classify(error),
        FileServiceError::NotFound(_) => RuntimeError::Target(TargetFailure::MissingEvidence {
            requirement: EvidenceRequirement::ReferencedContent,
        }),
        FileServiceError::Unsupported(_) => unsupported(),
        FileServiceError::Integrity(_) => corrupt(MetadataFailure::InvalidChecksum),
        _ => RuntimeError::InvalidOutput,
    }
}

fn database_error(error: DbError) -> RuntimeError {
    match error {
        DbError::ReadCancelled => cancelled(),
        DbError::ReadTimedOut => RuntimeError::Target(TargetFailure::DeadlineExceeded),
        DbError::Io(error) => super::super::proof_state::classify(error),
        other => RuntimeError::Database(other),
    }
}

fn corrupt(reason: MetadataFailure) -> RuntimeError {
    RuntimeError::Target(TargetFailure::CorruptMetadata { reason })
}

fn cancelled() -> RuntimeError {
    RuntimeError::Target(TargetFailure::Cancelled {
        scope: CancellationScope::Target,
    })
}

fn unsupported() -> RuntimeError {
    RuntimeError::Target(TargetFailure::UnsupportedFeature {
        capability: CapabilityKind::ResourceRecords,
    })
}

fn missing_namespace() -> RuntimeError {
    RuntimeError::Target(TargetFailure::MissingEvidence {
        requirement: EvidenceRequirement::NamespacePage,
    })
}

fn serialized_bytes(value: &Value) -> Result<u64, RuntimeError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .map_err(|_| RuntimeError::InvalidOutput)
}
