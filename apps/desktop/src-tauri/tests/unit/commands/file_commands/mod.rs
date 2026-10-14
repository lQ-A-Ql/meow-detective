mod extract;
mod media;
mod support;
mod viewer;

use super::media::{media_data_url_for_file, media_range_for_file as media_range_for_file_inner};
use super::preview::{image_preview_for_file, text_preview_for_file};
use super::viewer::read_file_range_for_state;
use crate::state::AppState;
use transport::{
    dto::{
        MediaPreviewModeDto, MediaRangeRequestDto, MediaRangeResponseDto, MAX_VIEWER_RANGE_LENGTH,
    },
    CommandError,
};

fn media_range_for_file(
    state: &AppState,
    connection: &rusqlite::Connection,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    support::increment_media_range_call(
        &crate::commands::command_support::require_active_case(state)?.case_id,
    );
    media_range_for_file_inner(state, connection, request)
}
