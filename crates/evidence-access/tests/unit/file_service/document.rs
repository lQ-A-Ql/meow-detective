use super::*;
use rusqlite::Connection;
use std::io::Cursor;

fn make_sqlite_bytes() -> Vec<u8> {
    let tmp = tempfile::NamedTempFile::new().expect("temp db");
    {
        let conn = Connection::open(tmp.path()).expect("open temp db");
        conn.execute_batch(
            "CREATE TABLE logins(id INTEGER PRIMARY KEY, url TEXT, username TEXT, note BLOB);
             INSERT INTO logins(url, username, note) VALUES
               ('http://3w.jlzb.vip/login', 'admin', X'0102'),
               ('https://example.com', 'alice', NULL);
             CREATE TABLE meta(key TEXT, value TEXT);
             INSERT INTO meta VALUES ('schema', '1');",
        )
        .expect("seed temp db");
    }
    std::fs::read(tmp.path()).expect("read temp db bytes")
}

#[test]
fn sqlite_preview_lists_tables_and_rows() {
    let preview = preview_sqlite(&make_sqlite_bytes()).expect("sqlite preview");
    assert_eq!(preview.kind, "sqlite");
    assert_eq!(preview.summary, "2 tables");
    assert_eq!(preview.sections.len(), 2);

    let logins = &preview.sections[0];
    assert_eq!(logins.title, "Table: logins");
    assert_eq!(logins.lines[0], "id | url | username | note");
    assert!(logins.lines[1].contains("http://3w.jlzb.vip/login"));
    assert!(logins.lines[1].contains("admin"));
    assert!(logins.lines[1].contains("[blob 2 bytes]"));
    assert!(logins.lines[2].contains("NULL"));

    let meta = &preview.sections[1];
    assert_eq!(meta.title, "Table: meta");
    assert!(meta.lines[1].contains("schema"));
}

#[test]
fn sqlite_preview_rejects_garbage() {
    assert!(preview_sqlite(b"not a sqlite database").is_err());
}

#[test]
fn sqlite_preview_reports_row_truncation() {
    let tmp = tempfile::NamedTempFile::new().expect("temp db");
    {
        let conn = Connection::open(tmp.path()).expect("open temp db");
        conn.execute_batch("CREATE TABLE events(id INTEGER PRIMARY KEY, value TEXT);")
            .expect("create table");
        for index in 0..=MAX_SQLITE_ROWS {
            conn.execute(
                "INSERT INTO events(value) VALUES (?1)",
                [format!("event-{index}")],
            )
            .expect("insert row");
        }
    }

    let preview =
        preview_sqlite(&std::fs::read(tmp.path()).expect("read db")).expect("sqlite preview");

    assert!(preview.truncated);
    assert_eq!(
        preview.sections[0].table.as_ref().unwrap().rows.len(),
        MAX_SQLITE_ROWS
    );
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("row preview limited")));
}

fn make_zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, content) in entries {
            writer.start_file(*path, options).expect("start zip entry");
            std::io::Write::write_all(&mut writer, content.as_bytes()).expect("write zip entry");
        }
        writer.finish().expect("finish zip");
    }
    buffer.into_inner()
}

#[test]
fn docx_preview_extracts_paragraphs() {
    let bytes = make_zip_bytes(&[(
        "word/document.xml",
        r#"<?xml version="1.0"?><w:document><w:body>
           <w:p><w:r><w:t>Hello forensic</w:t></w:r></w:p>
           <w:p><w:r><w:t>Second &amp; line</w:t></w:r></w:p>
           </w:body></w:document>"#,
    )]);
    let preview =
        preview_office_part(&bytes, DocumentKind::Docx, office_docx_lines).expect("docx preview");
    assert_eq!(preview.kind, "docx");
    let lines = &preview.sections[0].lines;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "Hello forensic");
    assert_eq!(lines[1], "Second & line");
}

#[test]
fn docx_preview_does_not_mark_exact_line_limit_as_truncated() {
    let paragraphs = (0..MAX_SECTION_LINES)
        .map(|index| format!("<w:p><w:r><w:t>line {index}</w:t></w:r></w:p>"))
        .collect::<String>();
    let document = format!("<w:document><w:body>{paragraphs}</w:body></w:document>");
    let bytes = make_zip_bytes(&[("word/document.xml", &document)]);

    let preview =
        preview_office_part(&bytes, DocumentKind::Docx, office_docx_lines).expect("preview");

    assert_eq!(preview.sections[0].lines.len(), MAX_SECTION_LINES);
    assert!(!preview.truncated);
}

#[test]
fn docx_preview_marks_content_beyond_line_limit_as_truncated() {
    let paragraphs = (0..=MAX_SECTION_LINES)
        .map(|index| format!("<w:p><w:r><w:t>line {index}</w:t></w:r></w:p>"))
        .collect::<String>();
    let document = format!("<w:document><w:body>{paragraphs}</w:body></w:document>");
    let bytes = make_zip_bytes(&[("word/document.xml", &document)]);

    let preview =
        preview_office_part(&bytes, DocumentKind::Docx, office_docx_lines).expect("preview");

    assert_eq!(preview.sections[0].lines.len(), MAX_SECTION_LINES);
    assert!(preview.truncated);
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("paragraph preview limited")));
}

#[test]
fn docx_preview_marks_long_paragraph_as_truncated() {
    let long_text = "x".repeat(MAX_LINE_CHARS + 1);
    let document = format!(
        "<w:document><w:body><w:p><w:r><w:t>{long_text}</w:t></w:r></w:p></w:body></w:document>"
    );
    let bytes = make_zip_bytes(&[("word/document.xml", &document)]);

    let preview =
        preview_office_part(&bytes, DocumentKind::Docx, office_docx_lines).expect("preview");

    assert_eq!(preview.sections[0].lines[0].chars().count(), MAX_LINE_CHARS);
    assert!(preview.truncated);
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("characters per line")));
}

#[test]
fn xlsx_preview_resolves_shared_strings() {
    let bytes = make_zip_bytes(&[
        (
            "xl/workbook.xml",
            r#"<?xml version="1.0"?><workbook><sheets>
               <sheet name="Summary" sheetId="1" r:id="rId1"/>
               </sheets></workbook>"#,
        ),
        (
            "xl/sharedStrings.xml",
            r#"<?xml version="1.0"?><sst>
               <si><t>url</t></si><si><t>http://3w.jlzb.vip</t></si><si><t>admin</t></si>
               <si><t>A&#65;&#x42;&amp;</t></si>
               </sst>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0"?><Relationships>
               <Relationship Id="rId1" Target="worksheets/sheet1.xml"/>
               </Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<?xml version="1.0"?><worksheet><sheetData>
               <row><c t="s"><v>0</v></c><c t="s"><v>1</v></c><c t="s"><v>2</v></c><c t="s"><v>3</v></c></row>
               <row><c><v>42</v></c><c t="inlineStr"><is><t>plain</t></is></c></row>
               </sheetData></worksheet>"#,
        ),
    ]);
    let preview = preview_xlsx(&bytes).expect("xlsx preview");
    assert_eq!(preview.kind, "xlsx");
    assert_eq!(preview.summary, "1 sheets");
    let lines = &preview.sections[0].lines;
    assert_eq!(preview.sections[0].title, "Sheet: Summary");
    assert_eq!(lines[0], "url | http://3w.jlzb.vip | admin | AAB&");
    assert!(lines[1].contains("42"));
}

#[test]
fn xlsx_preview_preserves_sparse_coordinates_and_ignores_formula_text() {
    let bytes = make_zip_bytes(&[
        (
            "xl/workbook.xml",
            r#"<workbook><sheets><sheet name="Sparse" r:id="rId7"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships><Relationship Id="rId7" Target="worksheets/sheet7.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet7.xml",
            r#"<worksheet><sheetData><row r="1">
               <c r="C1"><f>40+2</f><v>42</v></c>
               <c r="E1" t="inlineStr"><is><t>tail</t></is></c>
               </row></sheetData></worksheet>"#,
        ),
    ]);

    let preview = preview_xlsx(&bytes).expect("xlsx preview");
    let table = preview.sections[0].table.as_ref().expect("table");

    assert_eq!(table.columns, vec!["A", "B", "C", "D", "E"]);
    assert_eq!(table.rows[0], vec!["", "", "42", "", "tail"]);
    assert!(!preview.sections[0].lines[0].contains("40+2"));
}

#[test]
fn xlsx_preview_reports_row_and_xml_truncation() {
    let rows = (0..=MAX_SECTION_LINES)
        .map(|index| format!(r#"<row><c r="A{}"><v>{index}</v></c></row>"#, index + 1))
        .collect::<String>();
    let worksheet = format!("<worksheet><sheetData>{rows}</sheetData></worksheet>");
    let bytes = make_zip_bytes(&[
        (
            "xl/workbook.xml",
            r#"<workbook><sheets><sheet name="Rows" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        ("xl/worksheets/sheet1.xml", &worksheet),
    ]);

    let preview = preview_xlsx(&bytes).expect("xlsx preview");

    assert!(preview.truncated);
    assert_eq!(
        preview.sections[0].table.as_ref().unwrap().rows.len(),
        MAX_SECTION_LINES
    );
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("row preview limited")));
}

#[test]
fn xlsx_preview_bounds_malicious_sparse_column_coordinates() {
    let bytes = make_zip_bytes(&[
        (
            "xl/workbook.xml",
            r#"<workbook><sheets><sheet name="Sparse" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet><sheetData><row>
               <c r="A1"><v>kept</v></c><c r="ZZZZZZ1"><v>bounded</v></c>
               </row></sheetData></worksheet>"#,
        ),
    ]);

    let preview = preview_xlsx(&bytes).expect("bounded xlsx preview");
    let table = preview.sections[0].table.as_ref().expect("table");

    assert_eq!(table.columns, vec!["A"]);
    assert_eq!(table.rows[0], vec!["kept"]);
    assert!(preview.truncated);
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("column preview limited")));
}

fn make_minimal_pdf() -> Vec<u8> {
    let stream = "BT /F1 24 Tf 100 700 Td (Hello PDF) Tj ET\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, body).as_bytes());
    }
    let xref_pos = pdf.len();
    let mut xref = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in &offsets {
        xref.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.extend_from_slice(xref.as_bytes());
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[test]
fn pdf_preview_extracts_page_text() {
    let preview = preview_pdf(&make_minimal_pdf()).expect("pdf preview");
    assert_eq!(preview.kind, "pdf");
    assert_eq!(preview.summary, "1 pages");
    assert_eq!(preview.sections.len(), 1);
    assert_eq!(preview.sections[0].title, "Page 1");
    assert!(
        preview.sections[0]
            .lines
            .iter()
            .any(|line| line.contains("Hello PDF")),
        "expected extracted text, got {:?}",
        preview.sections[0].lines
    );
}

#[test]
fn pdf_preview_rejects_garbage() {
    assert!(preview_pdf(b"not a pdf").is_err());
}

#[test]
fn pptx_preview_reads_slide_parts() {
    let bytes = make_zip_bytes(&[
        (
            "ppt/presentation.xml",
            r#"<?xml version="1.0"?><p:presentation><p:sldIdLst>
               <p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId1"/>
               </p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships>
               <Relationship Id="rId1" Target="slides/slide2.xml"/>
               <Relationship Id="rId2" Target="slides/slide9.xml"/>
               </Relationships>"#,
        ),
        (
            "ppt/slides/slide9.xml",
            r#"<?xml version="1.0"?><p:sld><p:cSld><p:spTree><p:sp>
               <p:txBody><a:p><a:r><a:t>Slide one title</a:t></a:r></a:p></p:txBody>
               </p:sp></p:spTree></p:cSld></p:sld>"#,
        ),
        (
            "ppt/slides/slide2.xml",
            r#"<?xml version="1.0"?><p:sld><p:cSld><p:spTree><p:sp>
               <p:txBody><a:p><a:r><a:t>Second slide body</a:t></a:r></a:p></p:txBody>
               </p:sp></p:spTree></p:cSld></p:sld>"#,
        ),
    ]);
    let preview = preview_pptx(&bytes).expect("pptx preview");
    assert_eq!(preview.kind, "pptx");
    assert_eq!(preview.summary, "2 slides");
    assert_eq!(preview.sections.len(), 2);
    assert_eq!(preview.sections[0].title, "Slide 1");
    assert_eq!(preview.sections[0].lines[0], "Slide one title");
    assert_eq!(preview.sections[1].lines[0], "Second slide body");
}

#[test]
fn pptx_preview_reports_paragraph_truncation() {
    let paragraphs = (0..=MAX_SECTION_LINES)
        .map(|index| format!("<a:p><a:r><a:t>line {index}</a:t></a:r></a:p>"))
        .collect::<String>();
    let slide = format!("<p:sld><p:cSld><p:spTree>{paragraphs}</p:spTree></p:cSld></p:sld>");
    let bytes = make_zip_bytes(&[
        (
            "ppt/presentation.xml",
            r#"<p:presentation><p:sldIdLst><p:sldId r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships><Relationship Id="rId1" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        ("ppt/slides/slide1.xml", &slide),
    ]);

    let preview = preview_pptx(&bytes).expect("pptx preview");

    assert_eq!(preview.sections[0].lines.len(), MAX_SECTION_LINES);
    assert!(preview.truncated);
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("paragraph preview limited")));
}

#[test]
fn docx_preview_surfaces_malformed_xml_warning() {
    let bytes = make_zip_bytes(&[(
        "word/document.xml",
        r#"<w:document><w:body><w:p><w:t>partial"#,
    )]);

    let preview =
        preview_office_part(&bytes, DocumentKind::Docx, office_docx_lines).expect("preview");

    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("XML parse warning")));
}
