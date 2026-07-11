use domain::{EntryStatus, NotebookEntryType};
use persistence_sqlite::repositories::notebook_repo::{NotebookEntryFilters, StepFilters};
use rusqlite::Connection;
use transport::dto::{
    InvestigationStepDto, ListInvestigationStepsRequest, ListNotebookEntriesRequest,
    NotebookEntryDto, NotebookEntryStatusDto, NotebookEntryTypeDto,
};

use super::{list_entries, list_steps, NotebookError};

pub fn list_entries_for_request(
    conn: &Connection,
    case_id: &str,
    request: ListNotebookEntriesRequest,
) -> Result<Vec<NotebookEntryDto>, NotebookError> {
    let filters = NotebookEntryFilters {
        entry_type: request.entry_type.map(entry_type_from_dto),
        status: request.status.map(status_from_dto),
        tags: Some(request.tags),
        search: request.search,
        limit: request.limit,
        offset: request.offset,
    };
    list_entries(conn, case_id, &filters)
}

pub fn list_steps_for_request(
    conn: &Connection,
    case_id: &str,
    request: ListInvestigationStepsRequest,
) -> Result<Vec<InvestigationStepDto>, NotebookError> {
    let filters = StepFilters {
        step_kind: request.step_kind,
        success: request.success,
        limit: request.limit,
        offset: request.offset,
    };
    list_steps(conn, case_id, &filters)
}

fn entry_type_from_dto(value: NotebookEntryTypeDto) -> NotebookEntryType {
    match value {
        NotebookEntryTypeDto::Observation => NotebookEntryType::Observation,
        NotebookEntryTypeDto::Hypothesis => NotebookEntryType::Hypothesis,
        NotebookEntryTypeDto::Finding => NotebookEntryType::Finding,
        NotebookEntryTypeDto::ActionItem => NotebookEntryType::ActionItem,
        NotebookEntryTypeDto::Conclusion => NotebookEntryType::Conclusion,
    }
}

fn status_from_dto(value: NotebookEntryStatusDto) -> EntryStatus {
    match value {
        NotebookEntryStatusDto::Draft => EntryStatus::Draft,
        NotebookEntryStatusDto::Reviewed => EntryStatus::Reviewed,
        NotebookEntryStatusDto::Final => EntryStatus::Final,
    }
}
