fn sanitize_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_sep = false;
    for ch in value.chars() {
        if ch.is_ascii_whitespace() {
            if !out.is_empty() && !prev_sep {
                out.push('_');
                prev_sep = true;
            }
        } else if ch.is_ascii_graphic() {
            out.push(ch);
            prev_sep = false;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "na".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn emit(
    code: &str,
    stage: &str,
    action: &str,
    session: &str,
    archive: &str,
    source: &str,
    retry: &str,
    reason: &str,
    err: &str,
) {
    eprintln!(
        "MOON_WARN code={} stage={} action={} session={} archive={} source={} retry={} reason={} err={}",
        sanitize_value(code),
        sanitize_value(stage),
        sanitize_value(action),
        sanitize_value(session),
        sanitize_value(archive),
        sanitize_value(source),
        sanitize_value(retry),
        sanitize_value(reason),
        sanitize_value(err),
    );
}

#[cfg(test)]
mod tests {
    use super::sanitize_value;

    #[test]
    fn sanitize_value_rewrites_whitespace() {
        assert_eq!(sanitize_value("a b\tc"), "a_b_c");
    }

    #[test]
    fn sanitize_value_falls_back_for_empty() {
        assert_eq!(sanitize_value("   "), "na");
    }
}
