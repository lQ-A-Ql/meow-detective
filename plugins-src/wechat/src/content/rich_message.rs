use serde_json::{Map, Value};

use super::xml;

pub fn enrich(local_type: i64, content: &str, attrs: &mut Map<String, Value>) {
    copy_projection(content, "xml", attrs);
    let kind = match local_type {
        3 => Some("image"),
        34 => Some("voice"),
        43 => Some("video"),
        47 => Some("sticker"),
        49 => Some("app"),
        _ => None,
    };
    if let Some(kind) = kind {
        attrs.insert("mediaKind".to_string(), Value::String(kind.to_string()));
    }
    let media = xml::media_items(content);
    if !media.is_empty() {
        attrs.insert("mediaItems".to_string(), Value::Array(media));
    }
    match local_type {
        3 => copy_element_attributes(
            content,
            "img",
            attrs,
            &[
                ("md5", "mediaMd5"),
                ("aeskey", "mediaAesKey"),
                ("cdnmidimgurl", "mediaUrl"),
                ("cdnthumburl", "thumbnailUrl"),
                ("length", "mediaSize"),
            ],
        ),
        34 => copy_element_attributes(
            content,
            "voicemsg",
            attrs,
            &[
                ("voicelength", "voiceDurationMs"),
                ("voiceformat", "voiceFormat"),
                ("voiceurl", "mediaUrl"),
                ("clientmsgid", "clientMessageId"),
            ],
        ),
        43 => copy_element_attributes(
            content,
            "videomsg",
            attrs,
            &[
                ("playlength", "videoDurationSeconds"),
                ("length", "mediaSize"),
                ("md5", "mediaMd5"),
                ("cdnvideourl", "mediaUrl"),
                ("cdnthumburl", "thumbnailUrl"),
            ],
        ),
        47 => copy_element_attributes(
            content,
            "emoji",
            attrs,
            &[
                ("md5", "mediaMd5"),
                ("cdnurl", "mediaUrl"),
                ("thumburl", "thumbnailUrl"),
                ("width", "mediaWidth"),
                ("height", "mediaHeight"),
            ],
        ),
        49 => copy_app_message(content, attrs),
        _ => {}
    }
}

pub fn enrich_source(source: &str, attrs: &mut Map<String, Value>) {
    copy_projection(source, "sourceXml", attrs);
    copy_source_identities(source, attrs);
}

pub fn enrich_packed_info(source: &str, attrs: &mut Map<String, Value>) {
    copy_projection(source, "packedInfoXml", attrs);
    copy_source_identities(source, attrs);
}

fn copy_source_identities(source: &str, attrs: &mut Map<String, Value>) {
    for (tag, key) in [
        ("sourceusername", "sourceUsername"),
        ("sourcedisplayname", "sourceDisplayName"),
        ("fromusername", "sourceUsername"),
        ("fromnickname", "sourceDisplayName"),
        ("replyusername", "replyUsername"),
        ("replynickname", "replyNickname"),
    ] {
        if attrs.contains_key(key) {
            continue;
        }
        if let Some(value) = xml::tag_text(source, tag) {
            attrs.insert(key.to_string(), Value::String(value));
        }
    }
}

fn copy_projection(content: &str, prefix: &str, attrs: &mut Map<String, Value>) {
    let Some(projection) = xml::project(content) else {
        return;
    };
    attrs.insert(format!("{prefix}Parsed"), Value::Bool(true));
    attrs.insert(format!("{prefix}Root"), Value::String(projection.root));
    if let Some(text) = projection.visible_text {
        attrs.insert(format!("{prefix}Text"), Value::String(text));
    }
    if !projection.fields.is_empty() {
        attrs.insert(format!("{prefix}Fields"), Value::Object(projection.fields));
    }
}

fn copy_app_message(content: &str, attrs: &mut Map<String, Value>) {
    let app = xml::first_element_object(
        content,
        "appmsg",
        &[
            ("type", "appMessageType"),
            ("title", "appTitle"),
            ("des", "appDescription"),
            ("url", "appUrl"),
            ("appname", "appName"),
            ("sourcedisplayname", "sourceDisplayName"),
            ("thumburl", "thumbnailUrl"),
        ],
    );
    for (key, value) in app {
        attrs.insert(key, value);
    }
    let object = xml::first_element_object(
        content,
        "refermsg",
        &[
            ("type", "type"),
            ("svrid", "serverId"),
            ("fromusr", "fromUsername"),
            ("chatusr", "chatUsername"),
            ("displayname", "displayName"),
            ("content", "content"),
        ],
    );
    if !object.is_empty() {
        attrs.insert("referencedMessage".to_string(), Value::Object(object));
    }
}

fn copy_element_attributes(
    content: &str,
    element: &str,
    attrs: &mut Map<String, Value>,
    mappings: &[(&str, &str)],
) {
    let attributes = xml::first_element_attributes(content, element);
    for (source, target) in mappings {
        if let Some(value) = attributes
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(source))
            .map(|(_, value)| value.clone())
        {
            if matches!(*target, "mediaUrl" | "thumbnailUrl") && xml::is_opaque_locator(&value) {
                let locator_key = match *target {
                    "mediaUrl" => "mediaLocator",
                    "thumbnailUrl" => "thumbnailLocator",
                    _ => unreachable!("guarded by matches above"),
                };
                attrs.insert(locator_key.to_string(), Value::String(value));
                attrs.insert(
                    format!("{locator_key}Kind"),
                    Value::String("opaqueHex".to_string()),
                );
            } else {
                attrs.insert((*target).to_string(), Value::String(value));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_and_app_metadata_are_structured() {
        let mut voice = Map::new();
        enrich(
            34,
            r#"<msg><voicemsg voicelength="3210" voiceformat="4"/></msg>"#,
            &mut voice,
        );
        assert_eq!(voice["mediaKind"], "voice");
        assert_eq!(voice["voiceDurationMs"], "3210");

        let mut app = Map::new();
        enrich(
            49,
            "<msg><appmsg><title>证据链接</title><url>https://example.invalid</url></appmsg></msg>",
            &mut app,
        );
        assert_eq!(app["appTitle"], "证据链接");
        assert_eq!(app["appUrl"], "https://example.invalid");
        assert_eq!(app["xmlParsed"], true);
        assert_eq!(app["xmlRoot"], "msg");
    }

    #[test]
    fn source_xml_preserves_official_account_reply_fields() {
        let mut attrs = Map::new();
        enrich_source(
            "<msg><sourceusername>gh_owner</sourceusername><sourcedisplayname>号主</sourcedisplayname><replyusername>wxid_reader</replyusername><replynickname>读者</replynickname><content>回复正文</content></msg>",
            &mut attrs,
        );
        assert_eq!(attrs["sourceUsername"], "gh_owner");
        assert_eq!(attrs["sourceDisplayName"], "号主");
        assert_eq!(attrs["replyUsername"], "wxid_reader");
        assert_eq!(attrs["replyNickname"], "读者");
        assert_eq!(attrs["sourceXmlText"], "回复正文");
    }

    #[test]
    fn opaque_image_locator_is_kept_as_metadata_not_media_url() {
        let locator = "30".repeat(96);
        let mut attrs = Map::new();
        enrich(
            3,
            &format!(r#"<msg><img md5="abcd" cdnmidimgurl="{locator}"/></msg>"#),
            &mut attrs,
        );
        assert!(attrs.get("mediaUrl").is_none());
        assert_eq!(attrs["mediaLocator"], locator);
        assert_eq!(attrs["mediaLocatorKind"], "opaqueHex");
        assert_eq!(attrs["mediaMd5"], "abcd");
    }
}
