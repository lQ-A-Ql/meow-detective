use serde_json::Value;
use std::collections::BTreeMap;

pub(super) const FAMILIES: &[&str] = &[
    "RegistryValue",
    "RegistryHardwareDevice",
    "RegistryNetworkAdapter",
    "BrowserHistory",
    "BrowserDownload",
    "BrowserCookie",
    "BrowserSessionTab",
    "BrowserPassword",
    "BrowserExtension",
    "EmailAccount",
    "EmailMessage",
    "LinuxSystemConfig",
    "RegistryInstalledSoftware",
];

pub(super) const SYSTEM_FIELDS: &[&str] = &[
    "computerName",
    "productName",
    "currentBuild",
    "currentVersion",
    "displayVersion",
    "installDate",
    "registeredOwner",
    "registeredOrganization",
    "productId",
    "timezone",
    "language",
];

const HARDWARE_FIELDS: &[&str] = &[
    "kind",
    "deviceId",
    "name",
    "manufacturer",
    "properties",
    "keyPath",
];
const NETWORK_FIELDS: &[&str] = &[
    "guid",
    "name",
    "description",
    "macAddress",
    "permanentMacAddress",
    "ipAddresses",
    "subnetMasks",
    "gateways",
    "dhcpEnabled",
    "dhcpServer",
    "dnsServers",
];
const ACCOUNT_FIELDS: &[&str] = &[
    "account",
    "email",
    "mailClient",
    "incomingServer",
    "outgoingServer",
    "incomingPort",
    "outgoingPort",
    "incomingSsl",
    "outgoingSsl",
];

pub(super) fn category(
    family: &str,
    attrs: &Value,
) -> Option<(usize, String, &'static [&'static str])> {
    if family == "LinuxSystemConfig"
        && attrs
            .get("overlayContext")
            .is_some_and(|value| !value.is_null())
    {
        return None;
    }
    Some(match family {
        "RegistryValue" if SYSTEM_FIELDS.contains(&text(attrs, "field")) => {
            (0, text(attrs, "hivePath").to_owned(), &[])
        }
        "RegistryHardwareDevice" => (
            1,
            format!("{}:{}", text(attrs, "kind"), text(attrs, "deviceId")),
            HARDWARE_FIELDS,
        ),
        "RegistryNetworkAdapter" => (
            1,
            format!("network:{}", text(attrs, "guid")),
            NETWORK_FIELDS,
        ),
        "RegistryInstalledSoftware" if is_browser(text(attrs, "displayName")) => (
            2,
            format!("software:{}", text(attrs, "displayName")),
            &["displayName", "version", "publisher"],
        ),
        family if family.starts_with("Browser") => (
            2,
            format!(
                "{}:{}:{}",
                text(attrs, "browser"),
                text(attrs, "profile"),
                text(attrs, "sourceProfile")
            ),
            &["browser", "profile", "sourceProfile"],
        ),
        "EmailAccount" => (
            3,
            format!(
                "account:{}:{}:{}",
                text(attrs, "mailClient"),
                text(attrs, "email"),
                text(attrs, "account")
            ),
            ACCOUNT_FIELDS,
        ),
        "EmailMessage" => (3, email_container(attrs), &["mailClient", "containerPath"]),
        "LinuxSystemConfig"
            if text(attrs, "configKind") == "textConfig"
                && infrastructure::paths::evidence_relative_path(text(attrs, "sourcePath"))
                    .ends_with("/etc/hostname") =>
        {
            (
                0,
                format!("linux:{}", text(attrs, "sourcePath")),
                &["line", "sourcePath"],
            )
        }
        "LinuxSystemConfig" if text(attrs, "configKind") == "osRelease" => (
            0,
            format!("linux:{}", text(attrs, "sourcePath")),
            &[
                "configKind",
                "prettyName",
                "osId",
                "versionId",
                "hostname",
                "sourcePath",
            ],
        ),
        _ => return None,
    })
}

fn is_browser(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["chrome", "firefox", "microsoft edge", "brave", "opera"]
        .iter()
        .any(|browser| name.contains(browser))
}

fn email_container(attrs: &Value) -> String {
    let container = text(attrs, "containerPath");
    if !container.is_empty() {
        return format!("container:{container}");
    }
    let path = text(attrs, "sourcePath").replace('\\', "/");
    let parent = path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or_default();
    format!("directory:{}:{parent}", text(attrs, "mailClient"))
}

pub(super) fn text<'a>(attrs: &'a Value, field: &str) -> &'a str {
    attrs.get(field).and_then(Value::as_str).unwrap_or_default()
}

pub(super) fn selected(attrs: &Value, names: &[&str]) -> (BTreeMap<String, String>, bool) {
    let mut partial = false;
    let mut fields = BTreeMap::new();
    for name in names {
        if let Some(value) = attrs.get(*name).filter(|value| !value.is_null()) {
            let value = match value {
                Value::String(value) => value.clone(),
                Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => {
                    value.to_string()
                }
                Value::Null => continue,
            };
            partial |= value.chars().count() > 1024;
            let field = if *name == "line" { "hostname" } else { *name };
            fields.insert(field.to_string(), value.chars().take(1024).collect());
        }
    }
    (fields, partial)
}

pub(super) fn system_fields(attrs: &Value) -> (BTreeMap<String, String>, bool) {
    let field = text(attrs, "field");
    selected(
        &serde_json::json!({field: text(attrs, "data"), "hivePath": text(attrs, "hivePath")}),
        &[field, "hivePath"],
    )
}
