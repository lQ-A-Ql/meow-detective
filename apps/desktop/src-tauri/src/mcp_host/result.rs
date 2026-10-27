use app_services::mcp_host_service::McpHostServiceError;
use transport::{dto::mcp_host::McpHostToolCallResultDto, ApiErrorDto, CommandError};

pub(super) fn failure(error: CommandError) -> McpHostToolCallResultDto {
    McpHostToolCallResultDto {
        success: false,
        data: None,
        error: Some(error.message.clone()),
        error_details: Some(ApiErrorDto {
            code: error.code,
            message: error.message,
            category: Some(error.category),
            details: error.details.map(|value| *value),
            recoverable: error.recoverable.unwrap_or(false),
            suggestion: error.suggestion,
        }),
    }
}

pub(super) fn coded(mut error: CommandError, code: &str) -> CommandError {
    error.code = code.into();
    error
}

pub(super) fn tool_response(
    result: Result<serde_json::Value, McpHostServiceError>,
) -> McpHostToolCallResultDto {
    let data = match result {
        Ok(data) => data,
        Err(error) => return failure(CommandError::from_typed_service_error(error)),
    };
    let mut budget = ResultBudget {
        remaining: 1024 * 1024,
        exceeded: false,
    };
    match serde_json::to_writer(&mut budget, &data) {
        Ok(()) => McpHostToolCallResultDto {
            success: true,
            data: Some(data),
            error: None,
            error_details: None,
        },
        Err(_) if budget.exceeded => failure(coded(
            CommandError::invalid_input(McpHostServiceError::ResultTooLarge.to_string()),
            "MCP_RESULT_TOO_LARGE",
        )),
        Err(error) => failure(CommandError::from_typed_service_error(error)),
    }
}

struct ResultBudget {
    remaining: usize,
    exceeded: bool,
}

impl std::io::Write for ResultBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(std::io::Error::other("MCP result exceeds byte limit"));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp_host/result.rs"]
mod tests;
