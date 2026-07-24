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
               </sst>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<?xml version="1.0"?><worksheet><sheetData>
               <row><c t="s"><v>0</v></c><c t="s"><v>1</v></c><c t="s"><v>2</v></c></row>
               <row><c><v>42</v></c><c t="inlineStr"><is><t>plain</t></is></c></row>
               </sheetData></worksheet>"#,
        ),
    ]);
    let preview = preview_xlsx(&bytes).expect("xlsx preview");
    assert_eq!(preview.kind, "xlsx");
    assert_eq!(preview.summary, "1 sheets");
    let lines = &preview.sections[0].lines;
    assert_eq!(preview.sections[0].title, "Sheet: Summary");
    assert_eq!(lines[0], "url | http://3w.jlzb.vip | admin");
    assert!(lines[1].contains("42"));
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
            r#"<?xml version="1.0"?><p:presentation><p:sldIdLst><p:sldId id="256"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/slides/slide1.xml",
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
