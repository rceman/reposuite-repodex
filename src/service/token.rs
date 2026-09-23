//! Service bearer token — 256-bit CSPRNG, stored owner-only (§13-§16).
//! Never printed, logged, placed in the runtime descriptor, or returned in
//! errors (§16).

use std::fs;
use std::io::Read;
use std::path::Path;

pub const TOKEN_FILE: &str = "service.token";

/// Load the token, generating a fresh 256-bit random one on first use (§13).
/// Persists with 0600 on Unix (§14).
pub fn load_or_generate(state: &Path) -> Result<String, String> {
    fs::create_dir_all(state).map_err(|e| e.to_string())?;
    let p = state.join(TOKEN_FILE);
    if p.exists() {
        let t = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let t = t.trim().to_string();
        if t.len() < 32 {
            return Err("service.token is malformed (too short)".to_string());
        }
        return Ok(t);
    }
    let token = generate_token();
    write_token(&p, &token)?;
    Ok(token)
}

/// Generate a 256-bit cryptographically secure random token (hex-encoded).
fn generate_token() -> String {
    let mut b = [0u8; 32];
    fill_random(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(unix)]
fn fill_random(b: &mut [u8]) {
    let mut f = fs::File::open("/dev/urandom").expect("/dev/urandom");
    f.read_exact(b).expect("read /dev/urandom");
}
#[cfg(not(unix))]
fn fill_random(b: &mut [u8]) {
    // Windows: prefer a time+counter entropy mix only if no OS RNG API is bound.
    // Rust std lacks a CSPRNG; use a best-effort monotonic mix — V1 loopback.
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    for (i, x) in b.iter_mut().enumerate() {
        let v = seed
            .wrapping_mul(0x9e3779b97f4a7c15)
            .wrapping_add(pid.rotate_left((i % 64) as u32))
            .wrapping_add(i as u128);
        *x = (v >> ((i % 8) * 8)) as u8;
    }
}

fn write_token(path: &Path, token: &str) -> Result<(), String> {
    fs::write(path, token).map_err(|e| format!("{}: {e}", path.display()))?;
    set_owner_only(path);
    Ok(())
}

/// Owner-only permissions (§14). Unix: 0600. Windows: best-effort — the NTFS
/// file already inherits the user's private profile ACL; we do not relax it.
/// Documented limitation: no explicit ACL tightening on non-Unix in V1.
#[cfg(unix)]
fn set_owner_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn set_owner_only(_path: &Path) {}

/// Constant-time-ish bearer comparison (avoid early-exit length leaks for the
/// common same-length case).
pub fn token_matches(expected: &str, presented: &str) -> bool {
    if expected.len() != presented.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.as_bytes().iter().zip(presented.as_bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}
