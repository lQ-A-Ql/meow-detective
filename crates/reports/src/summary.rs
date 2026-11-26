use crate::CsvExporter;
use infrastructure::text::html_escape;
use std::io::{self, Write};
use transport::{
    commands::ReportScopeDto,
    dto::{ReportDocumentDto, ReportFormatDto, ReportSectionDto},
};

pub struct SummaryReportExporter;

impl SummaryReportExporter {
    pub fn export(
        writer: &mut impl Write,
        document: &ReportDocumentDto,
        format: ReportFormatDto,
    ) -> io::Result<()> {
        if format == ReportFormatDto::Json {
            return serde_json::to_writer_pretty(writer, document).map_err(io::Error::other);
        }
        let rows = rows(document, &document.scope);
        match format {
            ReportFormatDto::Html => {
                writeln!(writer, "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>{}</title><body><h1>{}</h1><table><thead><tr><th>来源</th><th>章节</th><th>对象</th><th>字段</th><th>内容</th></tr></thead><tbody>", html_escape(&document.case_name), html_escape(&document.case_name))?;
                for row in rows {
                    writeln!(
                        writer,
                        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                        html_escape(&row[0]),
                        html_escape(&row[1]),
                        html_escape(&row[2]),
                        html_escape(&row[3]),
                        html_escape(&row[4])
                    )?;
                }
                writeln!(writer, "</tbody></table></body></html>")
            }
            ReportFormatDto::Csv => CsvExporter::export_rows(
                writer,
                &["source", "section", "entity", "field", "value"],
                &rows,
            ),
            ReportFormatDto::Markdown => {
                writeln!(
                    writer,
                    "# {}\n\n| 来源 | 章节 | 对象 | 字段 | 内容 |\n| --- | --- | --- | --- | --- |",
                    markdown(&document.case_name)
                )?;
                for row in rows {
                    writeln!(
                        writer,
                        "| {} |",
                        row.iter()
                            .map(|cell| markdown(cell))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    )?;
                }
                Ok(())
            }
            ReportFormatDto::Json => unreachable!(),
        }
    }
}

fn rows(document: &ReportDocumentDto, scope: &ReportScopeDto) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for (field, value) in [
        ("案件名称", document.case_name.as_str()),
        ("案件标识", document.case_id.as_str()),
        (
            "案件编号",
            document.case_number.as_deref().unwrap_or_default(),
        ),
        ("检验人员", document.examiner.as_deref().unwrap_or_default()),
        ("生成时间", document.generated_at.as_str()),
    ] {
        push(&mut rows, "", "案件", "", field, value);
    }
    for source in &document.sources {
        for (field, value) in [
            ("来源标识", source.data_source_id.as_str()),
            ("平台", source.platform.as_str()),
            ("数据代次", source.generation.as_str()),
            ("证据校验", source.hash_status.as_str()),
            (
                "摘要状态",
                if source.summary_available {
                    "已生成"
                } else {
                    "尚未提取"
                },
            ),
        ] {
            push(&mut rows, &source.name, "来源", "", field, value);
        }
        for (included, name, section) in [
            (
                scope.operating_system,
                "操作系统",
                &source.summary.operating_system,
            ),
            (scope.hardware, "物理设备", &source.summary.hardware),
            (scope.browsers, "浏览器", &source.summary.browsers),
            (scope.email, "邮件", &source.summary.email),
        ] {
            if included {
                section_rows(&mut rows, &source.name, name, section);
            }
        }
    }
    rows
}

fn section_rows(rows: &mut Vec<Vec<String>>, source: &str, name: &str, section: &ReportSectionDto) {
    push(
        rows,
        source,
        name,
        "",
        "记录统计",
        &section.record_count.to_string(),
    );
    push(
        rows,
        source,
        name,
        "",
        "展示对象",
        &section.entities.len().to_string(),
    );
    push(
        rows,
        source,
        name,
        "",
        "覆盖状态",
        if section.partial {
            "部分展示"
        } else if section.record_count == 0 {
            "未取得数据"
        } else {
            "完整"
        },
    );
    for (field, count) in &section.counts {
        push(
            rows,
            source,
            name,
            "",
            crate::labels::field(field),
            &count.to_string(),
        );
    }
    for (index, entity) in section.entities.iter().enumerate() {
        let identity = (index + 1).to_string();
        for (field, value) in &entity.fields {
            push(
                rows,
                source,
                name,
                &identity,
                crate::labels::field(field),
                value,
            );
        }
        for (field, value) in &entity.counts {
            push(
                rows,
                source,
                name,
                &identity,
                crate::labels::field(field),
                &value.to_string(),
            );
        }
    }
}

fn push(
    rows: &mut Vec<Vec<String>>,
    source: &str,
    section: &str,
    entity: &str,
    field: &str,
    value: &str,
) {
    rows.push(
        [source, section, entity, field, value]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
}

fn markdown(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
}
