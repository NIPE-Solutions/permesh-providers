// SPDX-License-Identifier: MIT
use crate::{VISIBILITY, error};

pub(super) fn error_code(code: &str) -> &'static str {
    match code {
        "configuration" => "protocol_error",
        "unauthorized" => "authentication",
        "forbidden" | "not_found" | "membership" => "permission_denied",
        "rate_limit" => "rate_limited",
        "transport" | "timeout" | "limit" | "discovery" | "unavailable" => "unavailable",
        _ => "internal",
    }
}
pub(super) fn limitations(values: &[String]) -> Vec<&'static str> {
    let mut codes = Vec::new();
    for value in values {
        let code = if VISIBILITY.contains(&value.as_str()) {
            "visibility_limited"
        } else if value.ends_with(&error("forbidden").message)
            || value.ends_with(&error("not_found").message)
        {
            "permission_denied"
        } else if value.ends_with(&error("rate_limit").message) {
            "rate_limited"
        } else if value.ends_with(&error("limit").message) {
            "page_limit"
        } else {
            "unknown"
        };
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    codes
}
