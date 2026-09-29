//! Operator (op) list.
//!
//! Operators are the only players allowed to run admin commands (`/ib`, `/give`,
//! `/op`, ...). The list is kept in memory and persisted to `test_world/ops.txt`, one
//! username per line (blank lines and lines starting with `#` are ignored). Names are
//! compared case-insensitively.
//!
//! Ways to give op:
//! - `/op <player>` in game, from an existing operator,
//! - `op <player>` typed in the server console (stdin), if the server has one,
//! - editing `test_world/ops.txt` by hand (restart the server afterward),
//! - the `MC173_OPS=name1,name2` environment variable (applied on every start).
//!
//! The server runs in offline mode (usernames are not authenticated), so anyone able to
//! connect with an operator's username gets operator rights. Only expose the server on
//! a trusted network.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::path::Path;
use std::io;
use std::fs;

use tracing::{info, warn};

/// Path of the persisted operator list, relative to the working directory.
const OPS_FILE: &str = "test_world/ops.txt";

fn ops() -> MutexGuard<'static, HashSet<String>> {
    static OPS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    OPS.get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        // A poisoned lock only means another thread panicked while holding it, the set
        // itself is still perfectly usable.
        .unwrap_or_else(|err| err.into_inner())
}

fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Return true if the given username is acceptable: 1 to 16 characters, only ASCII
/// letters, digits and underscores. This is also what protects the per-player save
/// files from path traversal (`../`) through a hostile username.
pub fn is_valid_username(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Load the operator list from disk and from the `MC173_OPS` environment variable.
/// Call this once at startup.
pub fn load() {

    let mut set = ops();

    match fs::read_to_string(OPS_FILE) {
        Ok(content) => {
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') && is_valid_username(line) {
                    set.insert(normalize(line));
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => warn!("failed to read {OPS_FILE}: {err}"),
    }

    if let Some(env_ops) = std::env::var_os("MC173_OPS") {
        for name in env_ops.to_string_lossy().split(',') {
            let name = name.trim();
            if is_valid_username(name) {
                set.insert(normalize(name));
            }
        }
    }

    info!("{} operator(s) loaded", set.len());

}

/// Return true if the given player is an operator.
pub fn is_op(name: &str) -> bool {
    ops().contains(&normalize(name))
}

/// Give operator rights to a player, returning false if they already had them.
pub fn add(name: &str) -> bool {
    let mut set = ops();
    let added = set.insert(normalize(name));
    if added {
        save(&set);
    }
    added
}

/// Remove operator rights from a player, returning false if they did not have them.
pub fn remove(name: &str) -> bool {
    let mut set = ops();
    let removed = set.remove(&normalize(name));
    if removed {
        save(&set);
    }
    removed
}

/// Return the sorted list of operators.
pub fn list() -> Vec<String> {
    let mut names = ops().iter().cloned().collect::<Vec<_>>();
    names.sort();
    names
}

/// Persist the set, atomically (write to a temporary file then rename).
fn save(set: &HashSet<String>) {

    let mut names = set.iter().map(String::as_str).collect::<Vec<_>>();
    names.sort();
    let mut content = names.join("\n");
    content.push('\n');

    let res = (|| -> io::Result<()> {
        if let Some(parent) = Path::new(OPS_FILE).parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = format!("{OPS_FILE}.tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, OPS_FILE)?;
        Ok(())
    })();

    if let Err(err) = res {
        warn!("failed to save {OPS_FILE}: {err}");
    }

}
