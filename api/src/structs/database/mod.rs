use cap_std::ambient_authority;
use cap_std::fs::Dir;
use serde_json::Value;
use std::path::Path;

pub mod account;
pub mod account_id;
pub mod community;
pub mod counter;
pub mod data;
pub mod data_defaults;
pub mod global;
pub mod leaderboard;
pub mod write_key;

pub fn load_fallback_json(
    account: &str,
    project: &str,
    file_name_options: &[&str],
) -> Option<serde_json::Map<String, Value>> {
    for base in &["static", "webassets", "../static", "../webassets"] {
        if let Ok(dir) = Dir::open_ambient_dir(base, ambient_authority()) {
            for file_name in file_name_options {
                let rel_path = Path::new(account).join(project).join(file_name);
                if let Ok(content) = dir.read_to_string(&rel_path)
                    && let Ok(Value::Object(map)) = serde_json::from_str(&content)
                {
                    tracing::debug!("Loaded fallback JSON from: {:?}/{:?}", base, rel_path);
                    return Some(map);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_fallback_json_traversal_blocked() {
        // Traversal attempts must not escape static/webassets
        let res = load_fallback_json("..", "..", &["Cargo.toml"]);
        assert!(res.is_none());

        let res = load_fallback_json("../..", "api", &["Cargo.toml"]);
        assert!(res.is_none());
    }
}
