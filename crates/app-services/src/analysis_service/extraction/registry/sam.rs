use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(super) fn sam_user_artifacts(
    candidate: &EvidenceCandidate,
    info: &artifacts_windows::SamInfo,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    for user in &info.users {
        let rid_hex = format!("{:08x}", user.rid);
        let mut attrs = base_attrs(candidate);
        attrs.insert("username".to_string(), Value::String(user.username.clone()));
        attrs.insert("rid".to_string(), Value::Number(user.rid.into()));
        attrs.insert("ridHex".to_string(), Value::String(rid_hex.clone()));
        attrs.insert("sid".to_string(), Value::String(user.sid.clone()));
        attrs.insert("subjectSid".to_string(), Value::String(user.sid.clone()));
        attrs.insert(
            "subjectUsername".to_string(),
            Value::String(user.username.clone()),
        );
        attrs.insert(
            "groups".to_string(),
            Value::Array(
                user.group_memberships
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        attrs.insert(
            "loginCount".to_string(),
            Value::Number(user.login_count.into()),
        );
        if let Some(ts) = user.last_login {
            attrs.insert("lastLogin".to_string(), Value::String(ts.to_rfc3339()));
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "REGISTRY_LAST_LOGIN",
                ts,
                format!("SAM last login: {}", user.username),
                format!("User {} (RID {}) last logged in", user.username, user.rid),
                attrs.clone(),
                "registry.sam.v1",
            ));
        }
        if let Some(ts) = user.password_last_set {
            attrs.insert("accountCreated".to_string(), Value::String(ts.to_rfc3339()));
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "REGISTRY_ACCOUNT_CREATED",
                ts,
                format!("SAM account created: {}", user.username),
                format!(
                    "User {} (RID {}) account created/password set",
                    user.username, user.rid
                ),
                attrs.clone(),
                "registry.sam.v1",
            ));
        }
        let status = if user.account_locked {
            "locked"
        } else if user.account_disabled {
            "disabled"
        } else {
            "enabled"
        };
        attrs.insert(
            "accountStatus".to_string(),
            Value::String(status.to_string()),
        );
        if !user.profile_path.is_empty() {
            attrs.insert(
                "profilePath".to_string(),
                Value::String(user.profile_path.clone()),
            );
        }
        if let Some(hash) = &user.password_hash {
            attrs.insert("passwordHash".to_string(), Value::String(hash.clone()));
        }
        if let Some(hash_type) = &user.password_hash_type {
            attrs.insert(
                "passwordHashType".to_string(),
                Value::String(hash_type.clone()),
            );
        }
        attrs.insert(
            "parser".to_string(),
            Value::String("registry.sam".to_string()),
        );
        outcome.artifacts.push(make_artifact(
            "RegistrySamUser",
            format!("SAM User: {}", user.username),
            format!(
                "Local account {} (RID {}) status {}",
                user.username, user.rid, status
            ),
            candidate,
            "registry.sam.v1",
            attrs,
        ));
    }
    outcome
}
