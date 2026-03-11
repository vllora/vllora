/// Detect the provider name from an API key based on known prefix patterns.
///
/// Returns `Some(provider_name)` if the key matches a known prefix, or `None`
/// for keys without distinctive prefixes (e.g. TogetherAI, DeepInfra, MistralAI, ZAI, DeepSeek).
pub fn detect_provider_from_key(api_key: &str) -> Option<&'static str> {
    // Order matters: check longer/more specific prefixes before shorter ones
    // to avoid false matches (e.g. "sk-ant-" before generic "sk-")
    if api_key.starts_with("sk-ant-") {
        return Some("anthropic");
    }
    if api_key.starts_with("sk-or-") {
        return Some("openrouter");
    }
    if api_key.starts_with("AIza") {
        return Some("gemini");
    }
    if api_key.starts_with("xai-") {
        return Some("xai");
    }
    if api_key.starts_with("fw_") {
        return Some("fireworksai");
    }
    if api_key.starts_with("psk-") {
        return Some("parasail");
    }
    if api_key.starts_with("gsk_") {
        return Some("groq");
    }
    // Generic "sk-" fallback -> OpenAI (after ruling out sk-ant-, sk-or-)
    if api_key.starts_with("sk-") {
        return Some("openai");
    }
    None
}

/// Extract the raw API key from an Authorization header value.
///
/// Expects the format `Bearer <key>`. Returns `None` if the header
/// doesn't follow this format.
pub fn extract_bearer_token(authorization_header: &str) -> Option<&str> {
    authorization_header
        .strip_prefix("Bearer ")
        .filter(|key| !key.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn random_string_key(prefix: &str) -> String {
        let mut rng = rand::rng();
        let key: i32 = rng.random_range(1000000..9999999);
        format!("{prefix}-{key}")
    }
    #[test]
    fn test_detect_anthropic() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("sk-ant-api03")),
            Some("anthropic")
        );
    }

    #[test]
    fn test_detect_openrouter() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("sk-or-v1")),
            Some("openrouter")
        );
    }

    #[test]
    fn test_detect_gemini() {
        assert_eq!(
            detect_provider_from_key("AIzaSyBZadsadasPvwnLf4k"),
            Some("gemini")
        );
    }

    #[test]
    fn test_detect_xai() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("xai")),
            Some("xai")
        );
    }

    #[test]
    fn test_detect_fireworksai() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("fw")),
            Some("fireworksai")
        );
    }

    #[test]
    fn test_detect_parasail() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("psk-lang")),
            Some("parasail")
        );
    }

    #[test]
    fn test_detect_openai_generic_sk() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("sk-svcacct")),
            Some("openai")
        );
    }

    #[test]
    fn test_detect_openai_simple_sk() {
        assert_eq!(
            detect_provider_from_key(&random_string_key("sk-proj")),
            Some("openai")
        );
    }

    #[test]
    fn test_no_match_hex_string() {
        // TogetherAI-style key (no distinctive prefix)
        assert_eq!(
            detect_provider_from_key(
                "185785ebddeeb889bc95ade7e83801d4269a9ffb2f8be23771ae91a0c0f9fdcf"
            ),
            None
        );
    }

    #[test]
    fn test_no_match_alphanumeric() {
        // DeepInfra-style key
        assert_eq!(
            detect_provider_from_key("l8mm7n6wY2nGWZdMBQcZRp34zAwlo4EV"),
            None
        );
    }

    #[test]
    fn test_no_match_empty() {
        assert_eq!(detect_provider_from_key(""), None);
    }

    #[test]
    fn test_extract_bearer_token_valid() {
        assert_eq!(
            extract_bearer_token("Bearer sk-test123"),
            Some("sk-test123")
        );
    }

    #[test]
    fn test_extract_bearer_token_no_prefix() {
        assert_eq!(extract_bearer_token("sk-test123"), None);
    }

    #[test]
    fn test_extract_bearer_token_empty_key() {
        assert_eq!(extract_bearer_token("Bearer "), None);
    }

    #[test]
    fn test_extract_bearer_token_lowercase() {
        // Only exact "Bearer " is supported
        assert_eq!(extract_bearer_token("bearer sk-test123"), None);
    }
}
