use domain::{EntryStatus, EvidenceCitation, NodeType, NotebookEntry, NotebookEntryType};
use persistence_sqlite::repositories::notebook_repo::InvestigationStep;
use transport::dto::{
    EvidenceCitationDto, GraphNodeTypeDto, InvestigationStepDto, NotebookEntryDto,
    NotebookEntryStatusDto, NotebookEntryTypeDto,
};

pub(super) fn entry_type_to_dto(entry_type: &NotebookEntryType) -> NotebookEntryTypeDto {
    match entry_type {
        NotebookEntryType::Observation => NotebookEntryTypeDto::Observation,
        NotebookEntryType::Hypothesis => NotebookEntryTypeDto::Hypothesis,
        NotebookEntryType::Finding => NotebookEntryTypeDto::Finding,
        NotebookEntryType::ActionItem => NotebookEntryTypeDto::ActionItem,
        NotebookEntryType::Conclusion => NotebookEntryTypeDto::Conclusion,
    }
}

pub(super) fn entry_type_from_dto(dto: &NotebookEntryTypeDto) -> NotebookEntryType {
    match dto {
        NotebookEntryTypeDto::Observation => NotebookEntryType::Observation,
        NotebookEntryTypeDto::Hypothesis => NotebookEntryType::Hypothesis,
        NotebookEntryTypeDto::Finding => NotebookEntryType::Finding,
        NotebookEntryTypeDto::ActionItem => NotebookEntryType::ActionItem,
        NotebookEntryTypeDto::Conclusion => NotebookEntryType::Conclusion,
    }
}

pub(super) fn status_to_dto(status: &EntryStatus) -> NotebookEntryStatusDto {
    match status {
        EntryStatus::Draft => NotebookEntryStatusDto::Draft,
        EntryStatus::Reviewed => NotebookEntryStatusDto::Reviewed,
        EntryStatus::Final => NotebookEntryStatusDto::Final,
    }
}

pub(super) fn status_from_dto(dto: &NotebookEntryStatusDto) -> EntryStatus {
    match dto {
        NotebookEntryStatusDto::Draft => EntryStatus::Draft,
        NotebookEntryStatusDto::Reviewed => EntryStatus::Reviewed,
        NotebookEntryStatusDto::Final => EntryStatus::Final,
    }
}

pub(super) fn node_type_to_dto(node_type: &NodeType) -> GraphNodeTypeDto {
    match node_type {
        NodeType::File => GraphNodeTypeDto::File,
        NodeType::Artifact => GraphNodeTypeDto::Artifact,
        NodeType::TimelineEvent => GraphNodeTypeDto::TimelineEvent,
        NodeType::Entity => GraphNodeTypeDto::Entity,
        NodeType::Lead => GraphNodeTypeDto::Lead,
        NodeType::NotebookEntry => GraphNodeTypeDto::NotebookEntry,
    }
}

pub(super) fn node_type_from_dto(dto: &GraphNodeTypeDto) -> NodeType {
    match dto {
        GraphNodeTypeDto::File => NodeType::File,
        GraphNodeTypeDto::Artifact => NodeType::Artifact,
        GraphNodeTypeDto::TimelineEvent => NodeType::TimelineEvent,
        GraphNodeTypeDto::Entity => NodeType::Entity,
        GraphNodeTypeDto::Lead => NodeType::Lead,
        GraphNodeTypeDto::NotebookEntry => NodeType::NotebookEntry,
    }
}

pub(super) fn entry_to_dto(entry: &NotebookEntry) -> NotebookEntryDto {
    NotebookEntryDto {
        id: entry.id.clone(),
        case_id: entry.case_id.clone(),
        parent_id: entry.parent_id.clone(),
        author: entry.author.clone(),
        entry_type: entry_type_to_dto(&entry.entry_type),
        title: entry.title.clone(),
        body_markdown: entry.body_markdown.clone(),
        tags: entry.tags.clone(),
        status: status_to_dto(&entry.status),
        created_at: entry.created_at.clone(),
        updated_at: entry.updated_at.clone(),
    }
}

pub(super) fn citation_to_dto(citation: &EvidenceCitation) -> EvidenceCitationDto {
    EvidenceCitationDto {
        id: citation.id.clone(),
        entry_id: citation.entry_id.clone(),
        target_node_type: node_type_to_dto(&citation.target_node_type),
        target_node_id: citation.target_node_id.clone(),
        display_label: citation.display_label.clone(),
        snippet: citation.snippet.clone(),
        cited_at: citation.cited_at.clone(),
    }
}

pub(super) fn step_to_dto(step: &InvestigationStep) -> InvestigationStepDto {
    InvestigationStepDto {
        id: step.id.clone(),
        case_id: step.case_id.clone(),
        step_kind: step.step_kind.clone(),
        params_json: step.params_json.clone(),
        timestamp: step.timestamp.clone(),
        duration_ms: step.duration_ms.unwrap_or(0) as u32,
        case_state_hash: step.case_state_hash.clone(),
        success: step.success.unwrap_or(true),
        error_code: step.error_code.clone(),
    }
}
