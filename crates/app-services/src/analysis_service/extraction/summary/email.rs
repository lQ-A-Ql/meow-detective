use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_artifact_rows, status_from_total,
};
use crate::analysis_service::extraction::attr_mapping::{
    attachment_details_attr, header_attr, optional_bool_attr, optional_string_attr, string_attr,
    string_vec_attr, u64_attr,
};
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{EmailExtractionSummaryDto, EmailMessageDto};

pub fn get_email_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<EmailExtractionSummaryDto, AnalysisServiceError> {
    let total = count_artifacts_by_type(conn, "EmailMessage")?;
    let rows = query_artifact_rows(conn, &["EmailMessage"], offset, limit)?;
    let messages = rows
        .into_iter()
        .map(|row| EmailMessageDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            sent_at: optional_string_attr(&row.attrs, "sentAt"),
            received_at: optional_string_attr(&row.attrs, "receivedAt"),
            from: string_attr(&row.attrs, "from"),
            to: string_vec_attr(&row.attrs, "to"),
            cc: string_vec_attr(&row.attrs, "cc"),
            bcc: string_vec_attr(&row.attrs, "bcc"),
            reply_to: optional_string_attr(&row.attrs, "replyTo"),
            return_path: optional_string_attr(&row.attrs, "returnPath"),
            subject: string_attr(&row.attrs, "subject"),
            message_id: string_attr(&row.attrs, "messageId"),
            in_reply_to: optional_string_attr(&row.attrs, "inReplyTo"),
            references: string_vec_attr(&row.attrs, "references"),
            attachments: string_vec_attr(&row.attrs, "attachments"),
            attachment_details: attachment_details_attr(&row.attrs, "attachmentDetails"),
            headers: header_attr(&row.attrs, "headers"),
            body_preview: string_attr(&row.attrs, "bodyPreview"),
            body_plain: optional_string_attr(&row.attrs, "bodyPlain"),
            body_html: optional_string_attr(&row.attrs, "bodyHtml"),
            x_mailer: optional_string_attr(&row.attrs, "xMailer"),
            x_originating_ip: optional_string_attr(&row.attrs, "xOriginatingIp"),
            container_path: optional_string_attr(&row.attrs, "containerPath"),
            message_class: optional_string_attr(&row.attrs, "messageClass"),
            attachment_count: u64_attr(&row.attrs, "attachmentCount"),
            is_deleted: optional_bool_attr(&row.attrs, "isDeleted"),
        })
        .collect::<Vec<_>>();
    Ok(EmailExtractionSummaryDto {
        status: status_from_total(total),
        total,
        messages,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}
