pub fn parse_address(address: &str) -> Result<(String, u16), String> {
    let parts: Vec<&str> = address.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid address format: {}. Expected format: host:port",
            address
        ));
    }

    let port = parts[0]
        .parse::<u16>()
        .map_err(|_| format!("Invalid port number: {}", parts[0]))?;
    let host = parts[1].to_string();

    Ok((host, port))
}

pub fn shorten_peer_id(peer_id: &str) -> String {
    if peer_id.len() <= 18 {
        return peer_id.to_string();
    }
    format!("{}****{}", &peer_id[..8], &peer_id[peer_id.len() - 6..])
}

pub fn simplify_multiaddr_peer_ids(addr: &str) -> String {
    let mut parts: Vec<String> = addr
        .split('/')
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();

    let mut i = 0;
    while i + 1 < parts.len() {
        if parts[i] == "p2p" {
            parts[i + 1] = shorten_peer_id(&parts[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    format!("/{}", parts.join("/"))
}

pub fn connection_id_sort_key(connection_id: &str) -> u64 {
    if let Ok(value) = connection_id.parse::<u64>() {
        return value;
    }

    let digits: String = connection_id
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u64>().unwrap_or(u64::MAX)
}

pub fn summarize_ping_error_message(message: &str, verbose: bool) -> String {
    if verbose {
        return message.to_string();
    }

    let lower = message.to_lowercase();
    if !lower.contains("failed to negotiate transport protocol") {
        return message.to_string();
    }

    let attempts_section = message
        .split_once("[")
        .and_then(|(_, rest)| rest.rsplit_once("]").map(|(inside, _)| inside));

    let Some(raw_attempts) = attempts_section else {
        return "Dial failed (transport negotiation failed, use -v for details)".to_string();
    };

    let attempts = raw_attempts
        .split(")(")
        .map(|part| part.trim_matches(|c| c == '(' || c == ')'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if attempts.is_empty() {
        return "Dial failed (transport negotiation failed, use -v for details)".to_string();
    }

    let mut refused = 0usize;
    let mut timed_out = 0usize;
    let mut relay_failed = 0usize;
    let mut other = 0usize;

    for attempt in &attempts {
        let content = attempt.to_lowercase();
        if content.contains("connection refused") {
            refused += 1;
        } else if content.contains("timed out") || content.contains("timeout") {
            timed_out += 1;
        } else if content.contains("relay failed")
            || content.contains("failed to connect to destination")
        {
            relay_failed += 1;
        } else {
            other += 1;
        }
    }

    let mut parts = Vec::new();
    if refused > 0 {
        parts.push(format!("refused={refused}"));
    }
    if timed_out > 0 {
        parts.push(format!("timeout={timed_out}"));
    }
    if relay_failed > 0 {
        parts.push(format!("relay_failed={relay_failed}"));
    }
    if other > 0 {
        parts.push(format!("other={other}"));
    }

    format!(
        "Dial failed (attempts={}, {}, use -v for full details)",
        attempts.len(),
        parts.join(", ")
    )
}
