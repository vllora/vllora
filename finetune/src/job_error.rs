//! Job-level error contract — §2.5 of the redesign.
//!
//! Track: A | Feature: 001-job-based-cli-api
//!
//! The five error codes every Layer B job-lifecycle endpoint may return.
//! This is a narrow, purpose-built enum — separate from `GatewayApiError`
//! (which is the gateway's catch-all) so the job contract can evolve
//! independently without breaking every handler in the codebase.
//!
//! Canonical wire shape matches `specs/001-job-based-cli-api/contracts/jobs.openapi.yaml`:
//! ```json
//! { "code": "INVALID_REQUEST", "message": "..." }
//! ```
//!
//! HTTP status mapping:
//! - `INVALID_REQUEST` → 400
//! - `UNAUTHORIZED`    → 401
//! - `FORBIDDEN`       → 403
//! - `NOT_FOUND`       → 404
//! - `CONFLICT`        → 409

use serde::{Deserialize, Serialize};

/// The five job-contract error codes. Serialised as SCREAMING_SNAKE strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobErrorCode {
    /// Malformed request body, missing required field, invalid enum value.
    InvalidRequest,
    /// Caller is not authenticated.
    Unauthorized,
    /// Caller authenticated but not permitted for this workflow / action.
    Forbidden,
    /// Workflow, job, or grader not found.
    NotFound,
    /// Cancellation requested for a terminal / non-cancellable job, or an
    /// idempotency-key collision with a non-matching payload.
    Conflict,
}

impl JobErrorCode {
    /// HTTP status code the handler must return.
    pub const fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
        }
    }

    /// Canonical SCREAMING_SNAKE string used in the `code` response field.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
        }
    }
}

/// JSON body every job-contract error returns. Shape is frozen by
/// `jobs.openapi.yaml § ErrorResponse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobErrorBody {
    pub code: String,
    pub message: String,
}

/// Typed job-lifecycle error. Implementors should carry a human-readable
/// message; conversion to `JobErrorBody` is infallible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    pub code: JobErrorCode,
    pub message: String,
}

impl JobError {
    pub fn new(code: JobErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::new(JobErrorCode::InvalidRequest, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(JobErrorCode::NotFound, msg)
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(JobErrorCode::Conflict, msg)
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(JobErrorCode::Unauthorized, msg)
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(JobErrorCode::Forbidden, msg)
    }

    /// Materialise the wire body. Cheap clone.
    pub fn to_body(&self) -> JobErrorBody {
        JobErrorBody {
            code: self.code.as_str().to_string(),
            message: self.message.clone(),
        }
    }

    pub fn http_status(&self) -> u16 {
        self.code.http_status()
    }
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for JobError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contract test — every documented code maps to the exact status and
    /// wire string specified in §2.5 + `jobs.openapi.yaml`. Locks the
    /// contract against accidental renames / reorderings.
    #[test]
    fn all_five_codes_map_to_exact_http_status_and_wire_string() {
        let matrix = [
            (JobErrorCode::InvalidRequest, 400_u16, "INVALID_REQUEST"),
            (JobErrorCode::Unauthorized, 401, "UNAUTHORIZED"),
            (JobErrorCode::Forbidden, 403, "FORBIDDEN"),
            (JobErrorCode::NotFound, 404, "NOT_FOUND"),
            (JobErrorCode::Conflict, 409, "CONFLICT"),
        ];
        for (code, http, wire) in matrix {
            assert_eq!(code.http_status(), http, "wrong HTTP for {code:?}");
            assert_eq!(code.as_str(), wire, "wrong wire string for {code:?}");
        }
    }

    #[test]
    fn error_body_round_trips_through_json() {
        for code in [
            JobErrorCode::InvalidRequest,
            JobErrorCode::Unauthorized,
            JobErrorCode::Forbidden,
            JobErrorCode::NotFound,
            JobErrorCode::Conflict,
        ] {
            let err = JobError::new(code, "sample");
            let body = err.to_body();
            let json = serde_json::to_string(&body).unwrap();
            let decoded: JobErrorBody = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.code, code.as_str());
            assert_eq!(decoded.message, "sample");
        }
    }

    #[test]
    fn constructors_set_the_expected_code() {
        assert_eq!(
            JobError::invalid_request("x").code,
            JobErrorCode::InvalidRequest
        );
        assert_eq!(JobError::not_found("x").code, JobErrorCode::NotFound);
        assert_eq!(JobError::conflict("x").code, JobErrorCode::Conflict);
        assert_eq!(
            JobError::unauthorized("x").code,
            JobErrorCode::Unauthorized
        );
        assert_eq!(JobError::forbidden("x").code, JobErrorCode::Forbidden);
    }
}
