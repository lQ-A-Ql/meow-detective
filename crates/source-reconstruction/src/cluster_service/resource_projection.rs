use serde_json::{json, Value};

pub(super) fn structural_response(object: &Value) -> Option<Value> {
    if let Some(items) = object.get("items").and_then(Value::as_array) {
        return Some(json!({
            "kind": "List",
            "items": items.iter().filter_map(structural_response).collect::<Vec<_>>(),
        }));
    }
    let kind = text(object, "/kind")?;
    if !matches!(
        kind,
        "Pod" | "Namespace" | "PersistentVolume" | "PersistentVolumeClaim" | "Node"
    ) {
        return None;
    }
    let owners = object.pointer("/metadata/ownerReferences").and_then(Value::as_array)
        .into_iter().flatten().map(|owner| json!({
            "uid": text(owner, "/uid"), "name": text(owner, "/name"), "kind": text(owner, "/kind"),
        })).collect::<Vec<_>>();
    let volumes = object.pointer("/spec/volumes").and_then(Value::as_array)
        .into_iter().flatten().map(|volume| json!({
            "name": text(volume, "/name"),
            "persistentVolumeClaim": {"claimName": text(volume, "/persistentVolumeClaim/claimName")},
        })).collect::<Vec<_>>();
    Some(json!({
        "kind": kind,
        "metadata": {
            "uid": text(object, "/metadata/uid"),
            "name": text(object, "/metadata/name"),
            "namespace": text(object, "/metadata/namespace"),
            "resourceVersion": text(object, "/metadata/resourceVersion"),
            "ownerReferences": owners,
        },
        "status": {"phase": text(object, "/status/phase")},
        "spec": {
            "nodeName": text(object, "/spec/nodeName"),
            "volumeName": text(object, "/spec/volumeName"),
            "volumes": volumes,
            "claimRef": {
                "uid": text(object, "/spec/claimRef/uid"),
                "name": text(object, "/spec/claimRef/name"),
                "namespace": text(object, "/spec/claimRef/namespace"),
            },
            "rbd": {"image": text(object, "/spec/rbd/image"), "pool": text(object, "/spec/rbd/pool")},
            "csi": {
                "driver": text(object, "/spec/csi/driver"),
                "volumeHandle": text(object, "/spec/csi/volumeHandle"),
                "volumeAttributes": {
                    "clusterID": text(object, "/spec/csi/volumeAttributes/clusterID"),
                    "fsName": text(object, "/spec/csi/volumeAttributes/fsName"),
                    "rootPath": text(object, "/spec/csi/volumeAttributes/rootPath"),
                    "staticVolume": text(object, "/spec/csi/volumeAttributes/staticVolume"),
                },
            },
        },
    }))
}

fn text<'a>(object: &'a Value, pointer: &str) -> Option<&'a str> {
    object.pointer(pointer).and_then(Value::as_str)
}
