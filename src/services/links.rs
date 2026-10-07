//! Checks that resource links in a generated path actually exist.
//!
//! Only clear failures count as broken: not found, gone, a server error, or
//! no answer at all. Sites that refuse bots (401/403/429) or HEAD requests
//! (405, retried with GET) still count as real pages.

use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use crate::model::{LearningPath, Subtopic};

const TIMEOUT: Duration = Duration::from_secs(8);
const PARALLEL: usize = 8;

fn checker() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .user_agent(concat!("Nuevette/", env!("CARGO_PKG_VERSION"), " (link check)"))
        .build()
        .into()
}

fn is_reachable_status(status: u16) -> bool {
    status < 400 || matches!(status, 401 | 403 | 429)
}

fn exists(agent: &ureq::Agent, url: &str) -> bool {
    match agent.head(url).call() {
        Ok(response) if response.status().as_u16() == 405 => agent
            .get(url)
            .call()
            .is_ok_and(|r| is_reachable_status(r.status().as_u16())),
        Ok(response) => is_reachable_status(response.status().as_u16()),
        Err(_) => false,
    }
}

/// The subset of `urls` that could not be reached.
pub fn broken(urls: &[String]) -> HashSet<String> {
    let agent = checker();
    let unique: Vec<&String> = urls.iter().collect::<HashSet<_>>().into_iter().collect();
    let mut broken = HashSet::new();
    for chunk in unique.chunks(PARALLEL) {
        thread::scope(|scope| {
            let checks: Vec<_> = chunk
                .iter()
                .map(|url| {
                    let agent = agent.clone();
                    scope.spawn(move || (!exists(&agent, url)).then(|| (*url).clone()))
                })
                .collect();
            broken.extend(checks.into_iter().filter_map(|check| check.join().ok().flatten()));
        });
    }
    broken
}

/// Removes unreachable `https://` resources from every subtopic. Returns how
/// many were removed.
pub fn drop_broken(path: &mut LearningPath) -> usize {
    drop_broken_in(path.topics.iter_mut().flat_map(|t| &mut t.subtopics))
}

/// [`drop_broken`] for any set of subtopics, e.g. one regenerated topic.
pub fn drop_broken_in<'a>(subtopics: impl IntoIterator<Item = &'a mut Subtopic>) -> usize {
    let mut subtopics: Vec<&mut Subtopic> = subtopics.into_iter().collect();
    let urls: Vec<String> = subtopics
        .iter()
        .flat_map(|s| &s.resources)
        .filter(|r| r.starts_with("http://") || r.starts_with("https://"))
        .cloned()
        .collect();
    if urls.is_empty() {
        return 0;
    }
    let broken = broken(&urls);
    let mut removed = 0;
    for sub in &mut subtopics {
        let before = sub.resources.len();
        sub.resources.retain(|r| !broken.contains(r));
        removed += before - sub.resources.len();
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_walls_count_as_reachable_but_missing_pages_do_not() {
        assert!(is_reachable_status(200) && is_reachable_status(301));
        assert!(is_reachable_status(403) && is_reachable_status(429));
        assert!(!is_reachable_status(404) && !is_reachable_status(410) && !is_reachable_status(500));
    }
}
