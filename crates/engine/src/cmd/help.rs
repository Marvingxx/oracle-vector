//! Help → project links (this app's GitHub repository). The commands return the URL; the
//! frontend opens it.

use serde_json::{Value, json};

use super::*;

/// The app's id on GitHub.
pub const APP_ID: &str = "oracle-vector";
/// The GitHub account the Oracle apps live under.
pub const GITHUB_OWNER: &str = "Marvingxx";

/// This app's source repository.
pub fn github_url() -> String {
    format!("https://github.com/{GITHUB_OWNER}/{APP_ID}")
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "help.github", "Oracle Vector on GitHub", ["Help"], None, "{} → {url} source code, issues and releases", always, |_, _| url(github_url())),
        cmd!(query "help.links", "Links", [], None, "{} → {github}", always, |_, _| Ok(json!({ "github": github_url() }))),
    ]
}

fn url(u: String) -> Result<Value> {
    Ok(json!({ "url": u }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let mut s = Session::new();
        assert_eq!(s.execute("help.github", &json!({})).unwrap()["url"], "https://github.com/Marvingxx/oracle-vector");
        assert_eq!(s.execute("help.links", &json!({})).unwrap()["github"], "https://github.com/Marvingxx/oracle-vector");
        assert!(s.execute("help.discord", &json!({})).is_err());
    }
}
