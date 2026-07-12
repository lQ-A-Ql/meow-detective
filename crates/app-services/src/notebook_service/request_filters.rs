use persistence_sqlite::repositories::notebook_repo::{NotebookEntryFilters, StepFilters};
use rusqlite::Connection;
use transport::dto::{
    InvestigationStepDto, ListInvestigationStepsRequest, ListNotebookEntriesRequest,
    NotebookEntryDto,
};

use super::dto_conversion::{entry_type_from_dto, status_from_dto};
use super::{list_entries, list_steps, NotebookError};

pub fn list_entries_for_request(
    conn: &Connection,
    case_id: &str,
    request: ListNotebookEntriesRequest,
) -> Result<Vec<NotebookEntryDto>, NotebookError> {
    let filters = NotebookEntryFilters {
        entry_type: request.entry_type.as_ref().map(entry_type_from_dto),
        status: request.status.as_ref().map(status_from_dto),
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
