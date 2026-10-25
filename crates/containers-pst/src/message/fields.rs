use super::{Message, MessageHeader};
use chrono::DateTime;
use mail_parser::{Address, AddressList, HeaderForm, Headers, Mailbox};

pub(super) fn project(parsed: &mail_parser::Message<'_>) -> Message {
    let headers = parsed.headers();
    let first = parsed.from().and_then(|list| list.first());
    Message {
        subject: parsed.subject().unwrap_or_default().to_string(),
        from: addresses(parsed.from())
            .into_iter()
            .next()
            .unwrap_or_default(),
        sender_name: first
            .and_then(|mailbox| mailbox.name())
            .unwrap_or_default()
            .to_string(),
        sender_email: first
            .and_then(|mailbox| mailbox.address())
            .unwrap_or_default()
            .to_string(),
        to: addresses(parsed.to()),
        cc: addresses(parsed.cc()),
        bcc: addresses(parsed.bcc()),
        reply_to: header_text(headers, "Reply-To"),
        return_path: header_text(headers, "Return-Path"),
        message_id: header_text(headers, "Message-ID").unwrap_or_default(),
        in_reply_to: header_text(headers, "In-Reply-To"),
        references: parsed
            .references()
            .map(|list| list.iter().map(str::to_string).collect())
            .unwrap_or_default(),
        message_class: header_text(headers, "X-Message-Class"),
        x_mailer: header_text(headers, "X-Mailer"),
        x_originating_ip: header_text(headers, "X-Originating-IP"),
        date_raw: header_text(headers, "Date"),
        sent_time: parsed
            .date()
            .and_then(|date| DateTime::from_timestamp(date.to_timestamp(), 0)),
        received_time: headers
            .all("Received")
            .filter_map(|header| {
                header
                    .value()
                    .as_received()
                    .and_then(|received| received.date())
            })
            .last()
            .and_then(|date| DateTime::from_timestamp(date.to_timestamp(), 0)),
        headers: headers
            .into_iter()
            .map(|header| MessageHeader {
                name: header.raw_name().to_string(),
                value: decode_text(header.raw_value()),
                raw_value: header.raw_value().to_vec(),
                range: header.offset_field() as usize..header.offset_end() as usize,
            })
            .collect(),
        ..Message::default()
    }
}

fn header_text(headers: Headers<'_>, name: &str) -> Option<String> {
    headers
        .get(name)
        .map(|header| decode_text(header.raw_value()))
}

fn decode_text(bytes: &[u8]) -> String {
    let parsed = HeaderForm::Text.parse(bytes);
    parsed
        .value()
        .as_text()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn mailbox(value: Mailbox<'_>) -> String {
    match (value.name(), value.address()) {
        (Some(name), Some(address)) => format!("{name} <{address}>"),
        (_, Some(address)) => address.to_string(),
        (Some(name), _) => name.to_string(),
        _ => String::new(),
    }
}

fn addresses(list: Option<AddressList<'_>>) -> Vec<String> {
    list.map(|list| {
        list.iter()
            .map(|address| match address {
                Address::Mailbox(value) => mailbox(value),
                Address::Group(group) => format!(
                    "{}: {}",
                    group.name().unwrap_or_default(),
                    group
                        .mailboxes()
                        .map(mailbox)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })
            .collect()
    })
    .unwrap_or_default()
}
