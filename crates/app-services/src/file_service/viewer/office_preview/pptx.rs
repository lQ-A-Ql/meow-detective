use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::{events::Event, Reader};
use transport::dto::{DocumentPreviewDto, DocumentSectionDto};
use zip::ZipArchive;

use super::{office_pptx_lines, read_zip_text, resolve_zip_target, MAX_OFFICE_PARTS};
use crate::file_service::{viewer::document::DocumentKind, FileServiceError};

pub(crate) fn preview_pptx(bytes: &[u8]) -> Result<DocumentPreviewDto, FileServiceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        FileServiceError::invalid_input(format!("PPTX container parse failed: {error}"))
    })?;
    let presentation_path = "ppt/presentation.xml";
    let presentation = read_zip_text(&mut archive, presentation_path)?;
    let mut warnings = presentation.warnings(presentation_path);
    let slide_relationships = presentation_slide_relationships(&presentation.text);

    let rels_path = "ppt/_rels/presentation.xml.rels";
    let relationships = match read_zip_text(&mut archive, rels_path) {
        Ok(part) => {
            warnings.extend(part.warnings(rels_path));
            presentation_relationship_targets(&part.text)
        }
        Err(error) => {
            warnings.push(error.to_string());
            HashMap::new()
        }
    };

    let mut sections = Vec::new();
    let mut truncated = presentation.truncated || slide_relationships.len() > MAX_OFFICE_PARTS;
    for (index, relationship_id) in slide_relationships
        .iter()
        .take(MAX_OFFICE_PARTS)
        .enumerate()
    {
        let Some(target) = relationships.get(relationship_id) else {
            warnings.push(format!(
                "slide {}: unresolved relationship {relationship_id}",
                index + 1
            ));
            continue;
        };
        let Some(path) = resolve_zip_target("ppt", target).filter(|path| path.starts_with("ppt/"))
        else {
            warnings.push(format!(
                "slide {}: unsafe relationship target {target}",
                index + 1
            ));
            continue;
        };
        match read_zip_text(&mut archive, &path) {
            Ok(part) => {
                warnings.extend(part.warnings(&path));
                let text = office_pptx_lines(&part.text);
                truncated |= part.truncated || text.truncated;
                if text.line_count_truncated {
                    warnings.push(format!(
                        "slide {}: paragraph preview limited to {} lines",
                        index + 1,
                        crate::file_service::viewer::document::MAX_SECTION_LINES
                    ));
                }
                if text.line_width_truncated {
                    warnings.push(format!(
                        "slide {}: paragraph text limited to {} characters per line",
                        index + 1,
                        crate::file_service::viewer::document::MAX_LINE_CHARS
                    ));
                }
                sections.push(DocumentSectionDto {
                    title: format!("Slide {}", index + 1),
                    lines: text.lines,
                    table: None,
                });
            }
            Err(error) => warnings.push(format!("{path}: {error}")),
        }
    }

    Ok(DocumentPreviewDto {
        kind: DocumentKind::Pptx.as_str().to_string(),
        summary: format!("{} slides", slide_relationships.len()),
        sections,
        truncated,
        warnings,
    })
}

fn presentation_slide_relationships(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut relationships = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) | Ok(Event::Start(element))
                if element.name().as_ref() == b"p:sldId" || element.name().as_ref() == b"sldId" =>
            {
                if let Some(id) = element.attributes().flatten().find_map(|attribute| {
                    (attribute.key.as_ref() == b"r:id")
                        .then(|| String::from_utf8_lossy(&attribute.value).into_owned())
                }) {
                    relationships.push(id);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    relationships
}

fn presentation_relationship_targets(xml: &str) -> HashMap<String, String> {
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
