use url::Url;

/// Remove connection credentials and query values from errors returned to users.
pub fn redact_database_url_in_text(message: &str, database_url: &str) -> String {
    let mut redacted = message.replace(database_url, "[database URL redacted]");
    if let Ok(url) = Url::parse(database_url) {
        if let Some(password) = url.password().filter(|password| !password.is_empty()) {
            redacted = redacted.replace(password, "[redacted]");
            let decoded = percent_encoding::percent_decode_str(password).decode_utf8_lossy();
            redacted = redacted.replace(decoded.as_ref(), "[redacted]");
        }
        for (key, value) in url.query_pairs() {
            if !value.is_empty() {
                redacted =
                    redacted.replace(&format!("{key}={value}"), &format!("{key}=[redacted]"));
            }
        }
        for parameter in url.query().unwrap_or_default().split('&') {
            if let Some((key, value)) = parameter.split_once('=') {
                if !value.is_empty() {
                    redacted = redacted.replace(parameter, &format!("{key}=[redacted]"));
                }
            }
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_url_password_and_sensitive_query_values() {
        let url = "postgres://alice:p%40ssword@localhost:5432/app?sslmode=disable&token=secret123";
        let message = format!("failed at {url}; password=p@ssword; token=secret123");
        let sanitized = redact_database_url_in_text(&message, url);
        assert!(!sanitized.contains("p%40ssword"));
        assert!(!sanitized.contains("p@ssword"));
        assert!(!sanitized.contains("secret123"));
        assert!(sanitized.contains("[database URL redacted]"));
    }
}
