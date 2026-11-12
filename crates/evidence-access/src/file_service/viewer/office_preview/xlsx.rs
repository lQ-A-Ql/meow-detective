use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::{events::Event, Reader};
use transport::dto::{DocumentPreviewDto, DocumentSectionDto, DocumentTableDto};
use zip::ZipArchive;

use super::{read_zip_text, resolve_xml_reference, resolve_zip_target, xml_text, MAX_OFFICE_PARTS};
use crate::file_service::{viewer::document::bounded_line, FileServiceError};

use crate::file_service::viewer::document::{DocumentKind, MAX_SECTION_LINES};

const MAX_XLSX_PREVIEW_COLUMNS: usize = 256;

pub(crate) fn preview_xlsx(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("XLSX container parse failed: {error}"))
    })?;
    let workbook_path = "xl/workbook.xml";
    let workbook = read_zip_text(&mut archive, workbook_path)?;
    let mut warnings = workbook.warnings(workbook_path);
    let sheets = xlsx_sheet_entries(&workbook.text);

    let rels_path = "xl/_rels/workbook.xml.rels";
    let rels = match read_zip_text(&mut archive, rels_path) {
        Ok(part) => {
            warnings.extend(part.warnings(rels_path));
            xlsx_rels_targets(&part.text)
        }
        Err(error) => {
            warnings.push(error.to_string());
            HashMap::new()
        }
    };

    let shared_path = "xl/sharedStrings.xml";
    let shared = match read_zip_text(&mut archive, shared_path) {
        Ok(part) => {
            warnings.extend(part.warnings(shared_path));
            xlsx_shared_strings(&part.text)
        }
        Err(_) => Vec::new(),
    };

    let mut sections = Vec::new();
    let mut truncated = workbook.truncated || sheets.len() > MAX_OFFICE_PARTS;
    for (name, relationship_id) in sheets.iter().take(MAX_OFFICE_PARTS) {
        let Some(relationship_id) = relationship_id.as_deref() else {
            warnings.push(format!("sheet {name}: missing relationship id"));
            continue;
        };
        let Some(target) = rels.get(relationship_id) else {
            warnings.push(format!(
                "sheet {name}: unresolved relationship {relationship_id}"
            ));
            continue;
        };
        let Some(path) = resolve_zip_target("xl", target).filter(|path| path.starts_with("xl/"))
        else {
            warnings.push(format!("sheet {name}: unsafe relationship target {target}"));
            continue;
        };

        match read_zip_text(&mut archive, &path) {
            Ok(part) => {
                warnings.extend(part.warnings(&path));
                let parsed = xlsx_sheet_rows(&part.text, &shared);
                truncated |= part.truncated || parsed.rows_truncated || parsed.columns_truncated;
                if parsed.rows_truncated {
                    warnings.push(format!(
                        "sheet {name}: row preview limited to {MAX_SECTION_LINES} rows"
                    ));
                }
                if parsed.columns_truncated {
                    warnings.push(format!(
                        "sheet {name}: column preview limited to {MAX_XLSX_PREVIEW_COLUMNS} columns"
                    ));
                }
                let lines = parsed
                    .rows
                    .iter()
                    .map(|cells| bounded_line(&cells.join(" | ")))
                    .collect::<Vec<_>>();
                let width = parsed.rows.iter().map(Vec::len).max().unwrap_or(0);
                sections.push(DocumentSectionDto {
                    title: format!("Sheet: {name}"),
                    lines,
                    table: Some(DocumentTableDto {
                        columns: spreadsheet_column_names(width),
                        rows: parsed.rows,
                    }),
                });
            }
            Err(error) => warnings.push(format!("{path}: {error}")),
        }
    }

    Ok(DocumentPreviewDto {
        kind: DocumentKind::Xlsx.as_str().to_string(),
        summary: format!("{} sheets", sheets.len()),
        sections,
        truncated,
        warnings,
    })
}

fn xlsx_sheet_entries(xml: &str) -> Vec<(String, Option<String>)> {
    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.name().as_ref() == b"sheet" =>
            {
                let mut name = String::new();
                let mut relationship_id = None;
                for attribute in element.attributes().flatten() {
                    match attribute.key.as_ref() {
                        b"name" => name = String::from_utf8_lossy(&attribute.value).into_owned(),
                        b"r:id" => {
                            relationship_id =
                                Some(String::from_utf8_lossy(&attribute.value).into_owned())
                        }
                        _ => {}
                    }
                }
                entries.push((name, relationship_id));
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    entries
}

fn xlsx_rels_targets(xml: &str) -> HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut targets = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.name().as_ref() == b"Relationship" =>
            {
                let mut id = None;
                let mut target = None;
                let mut external = false;
                for attribute in element.attributes().flatten() {
                    match attribute.key.as_ref() {
                        b"Id" => id = Some(String::from_utf8_lossy(&attribute.value).into_owned()),
                        b"Target" => {
                            target = Some(String::from_utf8_lossy(&attribute.value).into_owned())
                        }
                        b"TargetMode" => {
                            external = attribute.value.as_ref().eq_ignore_ascii_case(b"External")
                        }
                        _ => {}
                    }
                }
                if !external {
                    if let (Some(id), Some(target)) = (id, target) {
                        targets.insert(id, target);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    targets
}

fn xlsx_shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if element.name().as_ref() == b"si" => {
                in_si = true;
                current.clear();
            }
            Ok(Event::Start(element)) if in_si && element.name().as_ref() == b"t" => {
                in_text = true;
            }
            Ok(Event::Text(text)) if in_text => {
                if let Some(value) = xml_text(&text) {
                    current.push_str(&value);
                }
            }
            Ok(Event::GeneralRef(reference)) if in_text => {
                if let Some(value) = resolve_xml_reference(reference.as_ref()) {
                    current.push_str(&value);
                }
            }
            Ok(Event::End(element)) if element.name().as_ref() == b"t" => in_text = false,
            Ok(Event::End(element)) if element.name().as_ref() == b"si" => {
                strings.push(std::mem::take(&mut current));
                in_si = false;
                in_text = false;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    strings
}

struct SheetRows {
    rows: Vec<Vec<String>>,
    rows_truncated: bool,
    columns_truncated: bool,
}

fn xlsx_sheet_rows(xml: &str, shared: &[String]) -> SheetRows {
    let mut reader = Reader::from_str(xml);
    let mut rows = Vec::new();
    let mut cells = Vec::new();
    let mut cell_type = String::new();
    let mut cell_value = String::new();
    let mut cell_column = None;
    let mut skip_cell = false;
    let mut in_value = false;
    let mut rows_truncated = false;
    let mut columns_truncated = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match element.name().as_ref() {
                b"row" => cells.clear(),
                b"c" => {
                    cell_value.clear();
                    cell_type.clear();
                    cell_column = None;
                    skip_cell = false;
                    for attribute in element.attributes().flatten() {
                        match attribute.key.as_ref() {
                            b"t" => {
                                cell_type = String::from_utf8_lossy(&attribute.value).into_owned()
                            }
                            b"r" => {
                                cell_column =
                                    column_index(&String::from_utf8_lossy(&attribute.value));
                                if cell_column.is_none() {
                                    skip_cell = true;
                                    columns_truncated = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                b"v" => in_value = true,
                b"t" if cell_type == "inlineStr" => in_value = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_value => {
                if let Some(value) = xml_text(&text) {
                    cell_value.push_str(&value);
                }
            }
            Ok(Event::GeneralRef(reference)) if in_value => {
                if let Some(value) = resolve_xml_reference(reference.as_ref()) {
                    cell_value.push_str(&value);
                }
            }
            Ok(Event::End(element)) => match element.name().as_ref() {
                b"v" | b"t" => in_value = false,
                b"c" => {
                    let rendered = render_cell_value(&cell_type, &mut cell_value, shared);
                    let column = cell_column.unwrap_or(cells.len());
                    if skip_cell || column >= MAX_XLSX_PREVIEW_COLUMNS {
                        columns_truncated = true;
                    } else {
                        if column >= cells.len() {
                            cells.resize(column + 1, String::new());
                        }
                        cells[column] = bounded_line(&rendered);
                    }
                    cell_value.clear();
                    cell_type.clear();
                    cell_column = None;
                    skip_cell = false;
                }
                b"row" => {
                    if cells.iter().any(|cell| !cell.is_empty()) {
                        if rows.len() == MAX_SECTION_LINES {
                            rows_truncated = true;
                            break;
                        }
                        rows.push(std::mem::take(&mut cells));
                    } else {
                        cells.clear();
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    SheetRows {
        rows,
        rows_truncated,
        columns_truncated,
    }
}

fn render_cell_value(cell_type: &str, cell_value: &mut String, shared: &[String]) -> String {
    if cell_type == "s" {
        cell_value
            .parse::<usize>()
            .ok()
            .and_then(|index| shared.get(index))
            .cloned()
            .unwrap_or_default()
    } else {
        std::mem::take(cell_value)
    }
}

fn column_index(reference: &str) -> Option<usize> {
    let mut value = 0usize;
    let mut seen = false;
    for byte in reference.bytes().take_while(u8::is_ascii_alphabetic) {
        seen = true;
        value = value.checked_mul(26)?;
        value = value.checked_add(usize::from(byte.to_ascii_uppercase() - b'A') + 1)?;
    }
    seen.then_some(value - 1)
}

fn spreadsheet_column_names(width: usize) -> Vec<String> {
    (0..width)
        .map(|index| {
            let mut remaining = index;
            let mut name = String::new();
            loop {
                name.insert(0, (b'A' + (remaining % 26) as u8) as char);
                remaining /= 26;
                if remaining == 0 {
                    break;
                }
                remaining -= 1;
            }
            name
        })
        .collect()
}
