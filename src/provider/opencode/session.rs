use rand::Rng;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

static SESSION_STATE: OnceLock<Mutex<(u64, u64)>> = OnceLock::new();
static PROXY_SESSION: OnceLock<String> = OnceLock::new();

/// Return one stable session for the proxy process. OpenCode accounts quota by
/// session, so minting a new id for every turn can exhaust free usage quickly.
pub fn proxy_session_id() -> &'static str {
    PROXY_SESSION.get_or_init(session_id)
}

/// Generate the OpenCode v2 session identifier shape accepted by free inference.
pub fn session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_millis() as u64;
    let state = SESSION_STATE.get_or_init(|| Mutex::new((0, 0)));
    let counter = {
        let mut state = state.lock().expect("session ID state poisoned");
        if state.0 != timestamp {
            state.0 = timestamp;
            state.1 = 0;
        }
        state.1 += 1;
        state.1
    };

    // OpenCode encodes the timestamp and a small per-millisecond counter in
    // the inverted 48-bit prefix; the free-tier gate validates that prefix.
    let value = !(timestamp.wrapping_mul(0x1000).wrapping_add(counter));
    let mut id = String::from("ses_");
    for shift in [40, 32, 24, 16, 8, 0] {
        id.push_str(&format!("{:02x}", (value >> shift) & 0xff));
    }

    let mut random = [0u8; 14];
    rand::rng().fill(&mut random);
    for byte in random {
        id.push(ALPHABET[(byte % 62) as usize] as char);
    }
    id
}

#[cfg(test)]
mod tests {
    use super::session_id;

    #[test]
    fn session_ids_have_expected_shape_and_differ() {
        let first = session_id();
        let second = session_id();
        assert!(first.starts_with("ses_"));
        assert_eq!(first.len(), 30);
        assert!(first[4..16]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        assert!(first[16..].bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert_ne!(first, second);
    }
}
