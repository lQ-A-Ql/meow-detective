use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const MAX_XML_NODES: usize = 2_048;
const MAX_XML_DEPTH: usize = 64;
const MAX_XML_FIELDS: usize = 512;
const MAX_DUPLICATE_VALUES: usize = 64;

pub struct Projection {
    pub root: String,
    pub visible_text: Option<String>,
    pub fields: Map<String, Value>,
}

#[derive(Default)]
struct XmlNode {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<XmlNode>,
}

impl XmlNode {
    fn named(&self, expected: &str) -> bool {
        self.name.eq_ignore_ascii_case(expected)
    }

    fn descendants_named<'a>(&'a self, expected: &str, out: &mut Vec<&'a XmlNode>) {
        if self.named(expected) {
            out.push(self);
        }
        for child in &self.children {
            child.descendants_named(expected, out);
        }
    }

    fn first_named(&self, expected: &str) -> Option<&XmlNode> {
        if self.named(expected) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.first_named(expected))
    }

    fn combined_text(&self) -> String {
        let mut parts = Vec::new();
        let own = self.text.trim();
        if !own.is_empty() {
            parts.push(own.to_string());
        }
        for child in &self.children {
            let text = child.combined_text();
            if !text.is_empty() {
                parts.push(text);
            }
        }
        parts.join(" ")
    }

    fn attribute(&self, expected: &str) -> Option<String> {
        self.attributes
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(expected))
            .map(|(_, value)| value.clone())
            .filter(|value| !value.trim().is_empty())
    }
}

pub fn tag_text(xml: &str, tag: &str) -> Option<String> {
    parse_nodes(xml)
        .iter()
        .find_map(|node| node.first_named(tag))
        .map(XmlNode::combined_text)
        .filter(|text| !text.is_empty())
}

pub fn first_element_attributes(xml: &str, element: &str) -> BTreeMap<String, String> {
    parse_nodes(xml)
        .iter()
        .find_map(|node| node.first_named(element))
        .map(|node| node.attributes.clone())
        .unwrap_or_default()
}

pub fn first_element_object(
    xml: &str,
    element: &str,
    fields: &[(&str, &str)],
) -> Map<String, Value> {
    let nodes = parse_nodes(xml);
    let Some(node) = nodes.iter().find_map(|node| node.first_named(element)) else {
        return Map::new();
    };
    let mut object = Map::new();
    for (tag, key) in fields {
        if let Some(value) = first_text(node, tag) {
            object.insert((*key).to_string(), Value::String(value));
        }
    }
    object
}

pub fn project(xml: &str) -> Option<Projection> {
    let roots = parse_nodes(xml);
    let root = roots.first()?;
    let mut fields = Map::new();
    let mut count = 0usize;
    for node in &roots {
        collect_fields(node, &node.name, &mut fields, &mut count);
        if count >= MAX_XML_FIELDS {
            break;
        }
    }
    Some(Projection {
        root: root.name.clone(),
        visible_text: visible_text(&roots),
        fields,
    })
}

pub fn media_items(xml: &str) -> Vec<Value> {
    let roots = parse_nodes(xml);
    let mut media = Vec::new();
    for root in &roots {
        root.descendants_named("media", &mut media);
    }
    media
        .into_iter()
        .take(512)
        .filter_map(media_object)
        .map(Value::Object)
        .collect()
}

pub fn interaction_items(xml: &str, element: &str) -> Vec<Value> {
    let roots = parse_nodes(xml);
    let mut interactions = Vec::new();
    for root in &roots {
        root.descendants_named(element, &mut interactions);
    }
    interactions
        .into_iter()
        .take(512)
        .filter_map(interaction_object)
        .map(Value::Object)
        .collect()
}

fn parse_nodes(xml: &str) -> Vec<XmlNode> {
    let mut reader = Reader::from_str(xml.trim_start_matches('\u{feff}'));
    reader.config_mut().check_end_names = false;
    let mut roots = Vec::new();
    let mut stack = Vec::new();
    let mut node_count = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if node_count < MAX_XML_NODES => {
                if stack.len() >= MAX_XML_DEPTH {
                    break;
                }
                stack.push(node_from_start(&element, reader.decoder()));
                node_count += 1;
            }
            Ok(Event::Empty(element)) if node_count < MAX_XML_NODES => {
                attach_node(
                    node_from_start(&element, reader.decoder()),
                    &mut stack,
                    &mut roots,
                );
                node_count += 1;
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    attach_node(node, &mut stack, &mut roots);
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(node) = stack.last_mut() {
                    if let Ok(decoded) = text.decode() {
                        if let Ok(unescaped) = quick_xml::escape::unescape(&decoded) {
                            node.text.push_str(&unescaped);
                        }
                    }
                }
            }
            Ok(Event::CData(text)) => {
                if let (Some(node), Ok(decoded)) = (stack.last_mut(), text.decode()) {
                    node.text.push_str(&decoded);
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                if let Some(node) = stack.last_mut() {
                    if let Some(value) = resolve_reference(reference.as_ref()) {
                        node.text.push_str(&value);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    while let Some(node) = stack.pop() {
        attach_node(node, &mut stack, &mut roots);
    }
    roots
}

fn node_from_start(element: &BytesStart<'_>, decoder: Decoder) -> XmlNode {
    let name = local_name(element.name().as_ref());
    let mut attributes = BTreeMap::new();
    for attribute in element.attributes().with_checks(false).flatten() {
        let key = local_name(attribute.key.as_ref());
        if let Ok(value) = attribute.decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
        {
            attributes.insert(key, value.into_owned());
        }
    }
    XmlNode {
        name,
        attributes,
        ..XmlNode::default()
    }
}

fn local_name(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':').next().unwrap_or(&name).to_string()
}

fn attach_node(node: XmlNode, stack: &mut [XmlNode], roots: &mut Vec<XmlNode>) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn resolve_reference(reference: &[u8]) -> Option<String> {
    let name = std::str::from_utf8(reference).ok()?;
    if let Some(value) = quick_xml::escape::resolve_predefined_entity(name) {
        return Some(value.to_string());
    }
    let code = name
        .strip_prefix("#x")
        .or_else(|| name.strip_prefix("#X"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .or_else(|| name.strip_prefix('#')?.parse::<u32>().ok())?;
    char::from_u32(code).map(|value| value.to_string())
}

fn first_text(node: &XmlNode, tag: &str) -> Option<String> {
    node.first_named(tag)
        .map(XmlNode::combined_text)
        .filter(|value| !value.is_empty())
}

fn text_or_attribute(node: &XmlNode, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        node.attribute(name)
            .or_else(|| first_text(node, name))
            .filter(|value| !value.trim().is_empty())
    })
}

fn media_object(node: &XmlNode) -> Option<Map<String, Value>> {
    let mut object = Map::new();
    for (names, key) in [
        (&["id", "mediaId"] as &[&str], "id"),
        (&["type", "mediaType"], "type"),
        (&["title", "name"], "title"),
        (&["description", "des", "desc"], "description"),
        (&["url", "cdnUrl", "cdnmidimgurl"], "url"),
        (&["thumb", "thumbUrl", "cdnthumburl"], "thumbUrl"),
        (&["md5", "mediaMd5"], "md5"),
        (&["aeskey", "aesKey"], "aesKey"),
    ] {
        if let Some(value) = text_or_attribute(node, names) {
            if matches!(key, "url" | "thumbUrl") && is_opaque_locator(&value) {
                object.insert(format!("{key}Locator"), Value::String(value));
                object.insert(format!("{key}Kind"), Value::String("opaqueHex".to_string()));
            } else {
                object.insert(key.to_string(), Value::String(value));
            }
        }
    }
    for (child_name, prefix) in [("url", "url"), ("thumb", "thumb")] {
        let Some(child) = node.children.iter().find(|child| child.named(child_name)) else {
            continue;
        };
        for (attribute, key) in [
            ("md5", format!("{prefix}Md5")),
            ("key", format!("{prefix}Key")),
            ("token", format!("{prefix}Token")),
            ("enc_idx", format!("{prefix}EncryptionIndex")),
        ] {
            if let Some(value) = child.attribute(attribute) {
                object.insert(key, Value::String(value));
            }
        }
    }
    if !object.contains_key("md5") {
        if let Some(value) = object.get("urlMd5").cloned() {
            object.insert("md5".to_string(), value);
        }
    }
    (!object.is_empty()).then_some(object)
}

/// WeChat sometimes stores a binary/CDN locator in a URL-shaped XML
/// attribute. It is an opaque evidence field, not image bytes and must never
/// be sent to a browser as a resource URL.
pub fn is_opaque_locator(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 64
        && value.len().is_multiple_of(2)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn interaction_object(node: &XmlNode) -> Option<Map<String, Value>> {
    let mut object = Map::new();
    for (names, key) in [
        (&["commentId", "commentID"] as &[&str], "commentId"),
        (&["username", "userName"], "username"),
        (&["nickname", "nickName"], "nickname"),
        (&["replyUsername", "replyUserName"], "replyUsername"),
        (&["replyNickname", "replyNickName"], "replyNickname"),
        (&["replyCommentId", "replyCommentID"], "replyCommentId"),
        (&["content", "commentContent"], "content"),
        (&["createTime", "timestamp"], "createTime"),
        (&["source", "commentFlag"], "source"),
    ] {
        if let Some(value) = text_or_attribute(node, names) {
            object.insert(key.to_string(), Value::String(value));
        }
    }
    if !object.contains_key("replyUsername") {
        if let Some(reply) = node
            .children
            .iter()
            .find(|child| child.named("replyUser") || child.named("replyTo"))
        {
            if let Some(value) = text_or_attribute(reply, &["username", "userName", "id"]) {
                object.insert("replyUsername".to_string(), Value::String(value));
            }
            if let Some(value) = text_or_attribute(reply, &["nickname", "nickName", "name"]) {
                object.insert("replyNickname".to_string(), Value::String(value));
            }
        }
    }
    (!object.is_empty()).then_some(object)
}

fn visible_text(roots: &[XmlNode]) -> Option<String> {
    if let Some(root) = roots.iter().find(|root| root.named("voipmsg")) {
        if let Some(value) = first_text(root, "msg") {
            return Some(value);
        }
    }
    const VISIBLE_TAGS: [&str; 10] = [
        "title",
        "des",
        "description",
        "content",
        "contentDesc",
        "displayname",
        "nickname",
        "wording",
        "plain",
        "digest",
    ];
    let mut values = Vec::new();
    for tag in VISIBLE_TAGS {
        if let Some(value) = roots.iter().find_map(|root| first_text(root, tag)) {
            if !values.contains(&value) {
                values.push(value);
            }
        }
        if values.len() >= 4 {
            break;
        }
    }
    (!values.is_empty()).then(|| values.join("\n"))
}

fn collect_fields(node: &XmlNode, path: &str, fields: &mut Map<String, Value>, count: &mut usize) {
    if *count >= MAX_XML_FIELDS {
        return;
    }
    for (name, value) in &node.attributes {
        insert_field(fields, format!("{path}.@{name}"), value.clone(), count);
    }
    let text = node.text.trim();
    if !text.is_empty() {
        let key = if node.children.is_empty() {
            path.to_string()
        } else {
            format!("{path}.#text")
        };
        insert_field(fields, key, text.to_string(), count);
    }
    for child in &node.children {
        collect_fields(child, &format!("{path}.{}", child.name), fields, count);
        if *count >= MAX_XML_FIELDS {
            break;
        }
    }
}

fn insert_field(fields: &mut Map<String, Value>, key: String, value: String, count: &mut usize) {
    use serde_json::map::Entry;
    match fields.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(Value::String(value));
            *count += 1;
        }
        Entry::Occupied(mut entry) => match entry.get_mut() {
            Value::Array(values) if values.len() < MAX_DUPLICATE_VALUES => {
                values.push(Value::String(value));
            }
            existing @ Value::String(_) => {
                let first = std::mem::replace(existing, Value::Null);
                *existing = Value::Array(vec![first, Value::String(value)]);
            }
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_repeated_media_cdata_and_numeric_entities() {
        let xml = "<mediaList><Media id=\"1\"><title><![CDATA[A &amp; B]]></title><url md5='abcd' key='9'>u&#x2f;1</url></Media><media id='2'/></mediaList>";
        let items = media_items(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["title"], "A &amp; B");
        assert_eq!(items[0]["url"], "u/1");
        assert_eq!(items[0]["md5"], "abcd");
        assert_eq!(items[0]["urlKey"], "9");
        assert_eq!(items[1]["id"], "2");
    }

    #[test]
    fn projects_nested_fields_and_duplicate_values() {
        let projection = project(
            "<msg><appmsg><title>主标题</title><item><content>一</content></item><item><content>二</content></item></appmsg></msg>",
        )
        .expect("projection");
        assert_eq!(projection.root, "msg");
        assert_eq!(projection.visible_text.as_deref(), Some("主标题\n一"));
        assert_eq!(
            projection.fields["msg.appmsg.item.content"],
            serde_json::json!(["一", "二"])
        );
    }

    #[test]
    fn preserves_reply_target_from_nested_or_mixed_case_xml() {
        let xml = "<CommentUser><UserName>owner</UserName><Content>号主回复</Content><ReplyUser><UserName>visitor</UserName><NickName>访客</NickName></ReplyUser></CommentUser>";
        let items = interaction_items(xml, "commentUser");
        assert_eq!(items[0]["username"], "owner");
        assert_eq!(items[0]["replyUsername"], "visitor");
        assert_eq!(items[0]["replyNickname"], "访客");
    }

    #[test]
    fn keeps_useful_nodes_from_an_incomplete_fragment() {
        let projection = project("<msg><title>仍可恢复</title><content>正文").expect("projection");
        assert_eq!(projection.fields["msg.title"], "仍可恢复");
        assert_eq!(projection.fields["msg.content"], "正文");
    }

    #[test]
    fn projects_voip_message_body_instead_of_raw_xml() {
        let projection = project(
            r#"<voipmsg type="VoIPBubbleMsg"><VoIPBubbleMsg><msg><![CDATA[已在其设备拒绝]]></msg><room_type>0</room_type></VoIPBubbleMsg></voipmsg>"#,
        )
        .expect("projection");
        assert_eq!(projection.visible_text.as_deref(), Some("已在其设备拒绝"));
    }

    #[test]
    fn classifies_long_hex_media_locator_without_treating_it_as_a_url() {
        let locator = "ab".repeat(48);
        let items = media_items(&format!("<media><url>{locator}</url></media>"));
        assert_eq!(items[0]["urlLocator"], locator);
        assert!(items[0].get("url").is_none());
        assert_eq!(items[0]["urlKind"], "opaqueHex");
        assert!(is_opaque_locator(&locator));
    }
}
