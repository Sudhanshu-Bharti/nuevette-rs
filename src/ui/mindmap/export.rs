//! A path as Markdown: a checklist that keeps progress, concepts,
//! prerequisites and resources, readable anywhere.

use crate::model::LearningPath;

pub fn to_markdown(path: &LearningPath) -> String {
    let (done, total) = path.progress();
    let mut out = format!("# {}\n\n{}\n\n", path.name, path.description);
    out.push_str(&format!("- **Estimated time:** {}\n", path.estimated_time));
    if let Some(level) = path.level {
        out.push_str(&format!("- **Level:** {}\n", level.label()));
    }
    out.push_str(&format!("- **Progress:** {done} of {total} subtopics done\n"));
    if let Some(source) = &path.source_url {
        out.push_str(&format!("- **Based on:** <{source}>\n"));
    }
    for (ti, topic) in path.topics.iter().enumerate() {
        out.push_str(&format!(
            "\n## {}. {} ({})\n\n{}\n\n",
            ti + 1,
            topic.name,
            topic.estimated_time,
            topic.description
        ));
        for (si, sub) in topic.subtopics.iter().enumerate() {
            let check = if path.is_done(ti, si) { "x" } else { " " };
            out.push_str(&format!(
                "- [{check}] **{}.{} {}** ({}): {}\n",
                ti + 1,
                si + 1,
                sub.name,
                sub.estimated_time,
                sub.description
            ));
            if !sub.technologies_and_concepts.is_empty() {
                out.push_str(&format!("  - Concepts: {}\n", sub.technologies_and_concepts.join(", ")));
            }
            if !sub.prerequisites.is_empty() {
                out.push_str(&format!("  - Prerequisites: {}\n", sub.prerequisites.join(", ")));
            }
            for resource in &sub.resources {
                if resource.starts_with("http") {
                    out.push_str(&format!("  - <{resource}>\n"));
                } else {
                    out.push_str(&format!("  - {resource}\n"));
                }
            }
        }
    }
    out
}

/// "Rust Async Programming" → "rust-async-programming.md"
pub fn file_name(path: &LearningPath) -> String {
    let slug: String = path
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{}.md", if slug.is_empty() { "learning-path" } else { &slug })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_paths, subtopic_key};

    #[test]
    fn markdown_is_a_checklist_with_progress() {
        let mut path = sample_paths().remove(0);
        path.completed.insert(subtopic_key(0, 0));
        let markdown = to_markdown(&path);
        assert!(markdown.starts_with("# Rust Async Programming\n"));
        assert!(markdown.contains("- **Progress:** 1 of 12 subtopics done"));
        assert!(markdown.contains("## 1. Foundations (~5 hours)"));
        assert!(markdown.contains("- [x] **1.1 Futures & the poll model** (~2 hours)"));
        assert!(markdown.contains("- [ ] **1.2 async / await syntax**"));
        assert!(markdown.contains("  - Concepts: Future, Poll, Waker"));
        assert!(markdown.contains("  - <https://rust-lang.github.io/async-book/>"));
    }

    #[test]
    fn file_names_are_slugs() {
        let mut path = sample_paths().remove(1);
        assert_eq!(file_name(&path), "next-js-14-app-router.md");
        path.name = "  ".into();
        assert_eq!(file_name(&path), "learning-path.md");
    }
}
