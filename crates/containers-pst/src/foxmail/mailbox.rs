const RECORD_SEPARATOR: [u8; 16] = [
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, b'S', b'\r',
    b'\n',
];

/// Result of splitting a Foxmail mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoxmailBox {
    /// RFC822 payloads that can be passed to the regular EML parser.
    pub records: Vec<Vec<u8>>,
    /// Number of Foxmail record separators observed in the source.
    pub separator_count: usize,
    /// Number of bounded records that did not contain an RFC822 header block.
    pub ignored_record_count: usize,
}

/// Split a Foxmail `.box` file into bounded RFC822 payloads.
pub fn parse_box(data: &[u8]) -> FoxmailBox {
    let positions = separator_positions(data);
    if positions.is_empty() {
        return FoxmailBox {
            records: Vec::new(),
            separator_count: 0,
            ignored_record_count: 0,
        };
    }

    let mut records = Vec::new();
    let mut ignored_record_count = 0;
    let mut segment_start = 0;
    for (separator_index, separator_start) in positions.iter().copied().enumerate() {
        collect_record(
            &data[segment_start..separator_start],
            &mut records,
            &mut ignored_record_count,
            separator_index > 0,
        );
        segment_start = separator_start + RECORD_SEPARATOR.len();
    }
    collect_record(
        &data[segment_start..],
        &mut records,
        &mut ignored_record_count,
        true,
    );

    FoxmailBox {
        records,
        separator_count: positions.len(),
        ignored_record_count,
    }
}

fn separator_positions(data: &[u8]) -> Vec<usize> {
    data.windows(RECORD_SEPARATOR.len())
        .enumerate()
        .filter_map(|(index, window)| (window == RECORD_SEPARATOR).then_some(index))
        .collect()
}

fn collect_record(
    segment: &[u8],
    records: &mut Vec<Vec<u8>>,
    ignored_record_count: &mut usize,
    count_invalid: bool,
) {
    let trimmed = trim_record_padding(segment);
    if trimmed.is_empty() {
        return;
    }
    let Some(header_start) = find_header_start(trimmed) else {
        if count_invalid {
            *ignored_record_count += 1;
        }
        return;
    };
    let payload = trim_record_padding(&trimmed[header_start..]);
    if payload.is_empty() {
        if count_invalid {
            *ignored_record_count += 1;
        }
    } else {
        records.push(payload.to_vec());
    }
}

fn trim_record_padding(mut data: &[u8]) -> &[u8] {
    while data
        .first()
        .is_some_and(|byte| *byte == 0 || *byte == b'\r' || *byte == b'\n')
    {
        data = &data[1..];
    }
    while data
        .last()
        .is_some_and(|byte| *byte == 0 || *byte == b'\r' || *byte == b'\n')
    {
        data = &data[..data.len() - 1];
    }
    data
}

fn find_header_start(data: &[u8]) -> Option<usize> {
    let mut line_start = 0;
    while line_start < data.len() {
        let line_end = data[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| line_start + offset)
            .unwrap_or(data.len());
        let line = data.get(line_start..line_end)?;
        if is_rfc822_header_line(line) {
            return Some(line_start);
        }
        if line_end == data.len() {
            break;
        }
        line_start = line_end + 1;
    }
    None
}

fn is_rfc822_header_line(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let Some(colon) = line.iter().position(|byte| *byte == b':') else {
        return false;
    };
    if colon == 0 || !line[..colon].iter().all(|byte| byte.is_ascii_graphic()) {
        return false;
    }
    let name = &line[..colon];
    if !name
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        return false;
    }
    name.eq_ignore_ascii_case(b"from")
        || name.eq_ignore_ascii_case(b"to")
        || name.eq_ignore_ascii_case(b"cc")
        || name.eq_ignore_ascii_case(b"bcc")
        || name.eq_ignore_ascii_case(b"subject")
        || name.eq_ignore_ascii_case(b"date")
        || name.eq_ignore_ascii_case(b"received")
        || name.eq_ignore_ascii_case(b"message-id")
        || name.eq_ignore_ascii_case(b"content-type")
        || name.eq_ignore_ascii_case(b"mime-version")
        || name.eq_ignore_ascii_case(b"content-transfer-encoding")
        || name.eq_ignore_ascii_case(b"content-disposition")
        || name
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"x-"))
}
