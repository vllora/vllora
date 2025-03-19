/// Custom HTTP status codes extending the standard ones.
pub mod status {
    use actix_web::http::StatusCode;

    use std::fmt;

    /// StatusCode 436 - Guard Validation Failed
    ///
    /// This status code indicates that a guard validation check has failed.
    /// It's a custom extension to the HTTP status codes for use with the guardrails system.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct GuardValidationFailed;

    impl GuardValidationFailed {
        /// The status code value for Guard Validation Failed
        pub const STATUS: u16 = 446;

        /// Creates a new StatusCode representing Guard Validation Failed (446)
        pub fn status_code() -> StatusCode {
            // Use the from_u16 method which will return a Result
            StatusCode::from_u16(Self::STATUS).expect("446 is a valid HTTP status code value")
        }

        /// Returns the reason phrase for this status code
        pub const fn reason_phrase() -> &'static str {
            "Guard Validation Failed"
        }
    }

    impl fmt::Display for GuardValidationFailed {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} {}", Self::STATUS, Self::reason_phrase())
        }
    }

    /// Extends the StatusCode with custom status code methods
    pub trait StatusCodeExt {
        /// Returns true if the status code is Guard Validation Failed (446)
        fn is_guard_validation_failed(&self) -> bool;
    }

    impl StatusCodeExt for StatusCode {
        fn is_guard_validation_failed(&self) -> bool {
            self.as_u16() == GuardValidationFailed::STATUS
        }
    }
}

/// Custom error responses
pub mod response {
    use super::*;
    use actix_web::{HttpResponse, ResponseError};
    use serde::Serialize;

    /// Error response for guard validation failures
    #[derive(Debug, Serialize)]
    pub struct GuardValidationError {
        pub message: String,
        pub guard_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub details: Option<serde_json::Value>,
    }

    impl std::fmt::Display for GuardValidationError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Guard validation failed: {}", self.message)
        }
    }

    impl ResponseError for GuardValidationError {
        fn status_code(&self) -> actix_web::http::StatusCode {
            status::GuardValidationFailed::status_code()
        }

        fn error_response(&self) -> HttpResponse {
            HttpResponse::build(self.status_code()).json(self)
        }
    }
}
