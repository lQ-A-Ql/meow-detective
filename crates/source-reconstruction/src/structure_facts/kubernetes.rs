use serde_json::{json, Value};

use super::{
    backend,
    model::{normalized_time, Evidence, FactProjection},
};
use crate::structure_analysis::StructureAnalysisError;

pub(super) fn project_file(
    path: &str,
    bytes: &[u8],
    source: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    if path.ends_with(".json") && path.contains("/etc/kubernetes/") {
        let object: Value = serde_json::from_slice(bytes)?;
        let mut evidence = source.clone();
        evidence.assertion_kind = "declared_configuration".into();
        return project_document(&object, &evidence, graph);
    }
    let input = std::str::from_utf8(bytes)
        .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
    let parsed = crate::cluster_service::parse_kubernetes_audit_log(input)
        .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
    if parsed.truncated || parsed.invalid_lines > 0 {
        return Err(StructureAnalysisError::Graph(
            "Kubernetes audit evidence is incomplete".into(),
        ));
    }
    for event in parsed.events {
        let Some(object) = event.resource_object else {
            continue;
        };
        if event.stage.as_deref() != Some("ResponseComplete")
            || !event
                .response_code
                .is_some_and(|code| (200..300).contains(&code))
        {
            continue;
        }
        for (expected, pointer) in [
            (event.object_uid.as_deref(), "/metadata/uid"),
            (event.name.as_deref(), "/metadata/name"),
            (event.namespace.as_deref(), "/metadata/namespace"),
        ] {
            if expected
                .is_some_and(|expected| !expected.is_empty() && text(&object, pointer) != expected)
            {
                return Err(StructureAnalysisError::Graph(
                    "Kubernetes audit resource identity conflicts with its response".into(),
                ));
            }
        }
        let mut evidence = source.clone();
        evidence.parser = "kubernetes.audit.response".into();
        evidence.assertion_kind = "observed_response".into();
        evidence.observed_at = normalized_time(event.stage_timestamp.as_deref())?;
        evidence.request_started_at = normalized_time(event.timestamp.as_deref())?;
        evidence.locator = Some(format!(
            "{}#line={};audit={}",
            path,
            event.line_number,
            event.audit_id.as_deref().unwrap_or_default()
        ));
        if evidence
            .request_started_at
            .as_ref()
            .zip(evidence.observed_at.as_ref())
            .is_some_and(|(start, end)| start > end)
        {
            return Err(StructureAnalysisError::Graph(
                "Kubernetes audit timestamps are reversed".into(),
            ));
        }
        project_document(&object, &evidence, graph)?;
    }
    Ok(())
}

fn project_document(
    object: &Value,
    evidence: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    if let Some(items) = object.get("items").and_then(Value::as_array) {
        if items.len() > 1024 {
            return Err(StructureAnalysisError::Graph(
                "Kubernetes resource bound exceeded".into(),
            ));
        }
        for (index, item) in items.iter().enumerate() {
            let mut evidence = evidence.clone();
            evidence.locator = Some(format!(
                "{};item={index}",
                evidence.locator.as_deref().unwrap_or_default()
            ));
            project_resource(item, &evidence, graph)?;
        }
        Ok(())
    } else {
        project_resource(object, evidence, graph)
    }
}

fn project_resource(
    object: &Value,
    evidence: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let kind = text(object, "/kind");
    if !matches!(
        kind,
        "Pod" | "Namespace" | "PersistentVolume" | "PersistentVolumeClaim" | "Node"
    ) {
        return Ok(());
    }
    let name = text(object, "/metadata/name");
    let uid = text(object, "/metadata/uid");
    let namespace = text(object, "/metadata/namespace");
    if name.is_empty() {
        return Ok(());
    }
    let scope = scope(evidence);
    let native = if uid.is_empty() {
        format!("name:{namespace}:{name}")
    } else {
        uid.into()
    };
    let entity_kind = match kind {
        "Namespace" => "namespace",
        "PersistentVolume" => "pv",
        "PersistentVolumeClaim" => "pvc",
        "Node" => "kubernetes_node",
        _ => "pod",
    };
    let resource = graph.entity(&scope, entity_kind, &native, name);
    let cluster = graph.entity(&scope, "kubernetes", "identity_unproven", "Kubernetes");
    graph.fact(
        &resource,
        Some(&cluster),
        "resource_in",
        evidence,
        json!({
            "uid": uid,
            "namespace": namespace,
            "resourceVersion": text(object, "/metadata/resourceVersion"),
            "identityState": if uid.is_empty() { "candidate" } else { "observed_uid" },
        }),
    )?;
    match kind {
        "Pod" => project_pod(object, &resource, evidence, graph)?,
        "PersistentVolume" => project_volume(object, &resource, evidence, graph)?,
        "PersistentVolumeClaim" => {
            let volume = text(object, "/spec/volumeName");
            if !uid.is_empty()
                && evidence.assertion_kind == "observed_response"
                && evidence.observed_at.is_some()
                && !volume.is_empty()
                && text(object, "/status/phase") == "Bound"
            {
                graph
                    .claims
                    .entry(resource.clone())
                    .or_default()
                    .push(backend::ObservedClaim {
                        volume_name: volume.into(),
                        namespace: namespace.into(),
                        evidence: evidence.clone(),
                    });
            }
            graph.fact(
                &resource,
                None,
                "declares_volume",
                evidence,
                json!({
                    "volumeName": volume,
                    "backendState": "needs_evidence",
                    "contentCapability": "metadata_only",
                }),
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn project_pod(
    object: &Value,
    pod: &str,
    evidence: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let scope = scope(evidence);
    let node_name = text(object, "/spec/nodeName");
    if !node_name.is_empty() {
        let node = graph.entity(
            &scope,
            "kubernetes_node",
            &format!("name::{node_name}"),
            node_name,
        );
        let fact = graph.fact(
            pod,
            Some(&node),
            "scheduled_on",
            evidence,
            json!({
                "nodeName": node_name,
                "resourceVersion": text(object, "/metadata/resourceVersion"),
                "validFrom": evidence.observed_at,
                "validUntil": evidence.observed_at,
            }),
        )?;
        if !text(object, "/metadata/uid").is_empty() {
            graph.placement(pod, &node, &fact, evidence);
        }
    }
    if let Some(owners) = object
        .pointer("/metadata/ownerReferences")
        .and_then(Value::as_array)
    {
        for owner in owners {
            let uid = text(owner, "/uid");
            if uid.is_empty() {
                continue;
            }
            let workload = graph.entity(&scope, "workload", uid, text(owner, "/name"));
            graph.fact(
                pod,
                Some(&workload),
                "owned_by",
                evidence,
                json!({"ownerKind": text(owner, "/kind")}),
            )?;
        }
    }
    if let Some(volumes) = object.pointer("/spec/volumes").and_then(Value::as_array) {
        for volume in volumes {
            let claim = text(volume, "/persistentVolumeClaim/claimName");
            if claim.is_empty() {
                continue;
            }
            let reference = graph.entity(
                &scope,
                "pvc",
                &format!("name:{}:{claim}", text(object, "/metadata/namespace")),
                claim,
            );
            graph.fact(
                pod,
                Some(&reference),
                "declares_claim",
                evidence,
                json!({
                    "backendState": "needs_evidence", "contentCapability": "metadata_only",
                }),
            )?;
        }
    }
    Ok(())
}

fn project_volume(
    object: &Value,
    volume: &str,
    evidence: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let spec = object.get("spec").unwrap_or(&Value::Null);
    let claim_uid = text(spec, "/claimRef/uid");
    let claim = if claim_uid.is_empty() {
        None
    } else {
        let claim = graph.entity(
            &scope(evidence),
            "pvc",
            claim_uid,
            text(spec, "/claimRef/name"),
        );
        graph.fact(
            &claim,
            Some(volume),
            "bound_volume_reference",
            evidence,
            json!({
                "claimUid": claim_uid,
                "namespace": text(spec, "/claimRef/namespace"),
            }),
        )?;
        (text(object, "/status/phase") == "Bound").then_some(claim)
    };
    graph.fact(
        volume,
        None,
        "declares_backend",
        evidence,
        json!({
            "driver": text(spec, "/csi/driver"),
            "volumeHandle": text(spec, "/csi/volumeHandle"),
            "rbdImage": text(spec, "/rbd/image"),
            "rbdPool": text(spec, "/rbd/pool"),
            "backendState": "needs_evidence", "contentCapability": "metadata_only",
        }),
    )?;
    backend::csi_request(
        graph,
        volume,
        claim,
        spec,
        text(object, "/metadata/name"),
        evidence,
    );
    Ok(())
}

fn text<'a>(object: &'a Value, pointer: &str) -> &'a str {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn scope(evidence: &Evidence) -> String {
    super::model::key(
        "kubernetes-scope",
        &[
            &evidence.source_revision.case_id,
            &evidence.acquisition_id,
            &evidence.source_revision.source_id,
        ],
    )
}
