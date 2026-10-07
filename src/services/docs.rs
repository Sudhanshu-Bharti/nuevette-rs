//! Finding and reading official documentation for a topic.
//!
//! The web app took the first Google hit and ~500 characters near a few
//! headings. Here the official site is chosen from the top results, and the
//! docs' own outline (their h1-h3 structure, plus up to two "getting started"
//! style pages) becomes Gemini's context, so paths follow the real docs.

use anyhow::{Context as _, Result, anyhow, bail};
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
use url::Url;

use super::redact;

/// Official docs for common topics, keyed by the topic with punctuation and
/// spaces removed ("Next.js" → "nextjs").
const KNOWN_DOCS: &[(&str, &str)] = &[
    ("react", "https://react.dev/learn"),
    ("nextjs", "https://nextjs.org/docs"),
    ("python", "https://docs.python.org/3/tutorial/"),
    ("rust", "https://doc.rust-lang.org/book/"),
    ("rustasync", "https://rust-lang.github.io/async-book/"),
    ("go", "https://go.dev/doc/"),
    ("golang", "https://go.dev/doc/"),
    ("typescript", "https://www.typescriptlang.org/docs/"),
    ("javascript", "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide"),
    ("kubernetes", "https://kubernetes.io/docs/home/"),
    ("docker", "https://docs.docker.com/get-started/"),
    ("postgresql", "https://www.postgresql.org/docs/current/tutorial.html"),
    ("postgres", "https://www.postgresql.org/docs/current/tutorial.html"),
    ("vue", "https://vuejs.org/guide/introduction.html"),
    ("svelte", "https://svelte.dev/docs"),
    ("django", "https://docs.djangoproject.com/en/stable/"),
    ("flutter", "https://docs.flutter.dev/"),
    ("swift", "https://docs.swift.org/swift-book/"),
    ("kotlin", "https://kotlinlang.org/docs/home.html"),
];

/// Hosts that are never the official documentation for anything.
const NOT_OFFICIAL: &[&str] = &[
    "medium.com", "dev.to", "stackoverflow.com", "reddit.com", "youtube.com", "quora.com",
    "geeksforgeeks.org", "w3schools.com", "tutorialspoint.com", "javatpoint.com",
    "freecodecamp.org", "udemy.com", "coursera.org", "linkedin.com", "wikipedia.org",
    "rust.docs.kernel.org", "programiz.com", "hashnode.dev", "substack.com",
];

/// Words in a link or path that suggest documentation.
const DOC_WORDS: &[&str] = &["docs", "doc", "documentation", "learn", "guide", "book", "manual", "reference"];

/// Link texts that point at the parts of the docs a learner reads first.
const START_WORDS: &[&str] = &[
    "getting started", "get started", "quick start", "quickstart", "tutorial", "introduction",
    "learn", "the basics", "overview", "fundamentals",
];

const SECTIONS: &[&str] = &[
    "Introduction",
    "Getting Started",
    "Core Concepts",
    "API Reference",
    "Advanced Guides",
];
const SECTION_WINDOW: usize = 500;
const FALLBACK_LEN: usize = 2000;
const MAX_OUTLINE: usize = 60;
const MAX_FOLLOW_UPS: usize = 2;
const MAX_CONTEXT: usize = 7000;

/// Elements whose text is never documentation content.
const SKIPPED_ELEMENTS: &[&str] = &["script", "style", "noscript", "svg", "template"];

/// Words that say nothing about what a topic is.
const STOP_WORDS: &[&str] = &[
    "for", "and", "the", "with", "how", "learn", "learning", "basics", "intro", "introduction",
    "beginners", "advanced", "guide", "course", "using", "from", "into", "ml", "ai", "js",
];

/// The words that identify a topic: "Linear algebra for ML" → ["linear", "algebra"].
pub fn topic_words(topic: &str) -> Vec<String> {
    topic
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() >= 3 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// Whether `text` is plausibly about the topic: it mentions most of the
/// topic's words. Guards against product sites that merely share a word.
pub fn is_relevant(topic: &str, text: &str) -> bool {
    let words = topic_words(topic);
    if words.is_empty() {
        return true;
    }
    let text = text.to_lowercase();
    let found = words.iter().filter(|w| text.contains(w.as_str())).count();
    found * 2 > words.len()
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn known_docs_url(topic: &str) -> Option<&'static str> {
    let key = normalized(topic);
    KNOWN_DOCS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, url)| *url)
}

#[derive(Clone, Debug, Deserialize)]
pub struct SearchResult {
    pub link: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Deserialize)]
struct SerpResponse {
    #[serde(default)]
    organic_results: Vec<SearchResult>,
    error: Option<String>,
}

/// The top Google results for "<topic> official documentation".
pub fn search_docs(agent: &ureq::Agent, topic: &str, api_key: &str) -> Result<Vec<SearchResult>> {
    let query = format!("{topic} official documentation");
    let result = (|| -> Result<Vec<SearchResult>> {
        let mut response = agent
            .get("https://serpapi.com/search.json")
            .query("engine", "google")
            .query("q", &query)
            .query("num", "10")
            .query("api_key", api_key)
            .call()?;
        let status = response.status();
        let body: SerpResponse = response
            .body_mut()
            .read_json()
            .with_context(|| format!("unexpected SerpAPI response (HTTP {status})"))?;
        if let Some(error) = body.error {
            bail!("SerpAPI: {error}");
        }
        if !status.is_success() {
            bail!("SerpAPI returned HTTP {status}");
        }
        Ok(body.organic_results)
    })();
    // The key travels in the query string, so make sure no error can echo it.
    result.map_err(|error| anyhow!(redact(format!("{error:#}"), api_key)))
}

/// Picks the result most likely to be the official docs. A candidate needs
/// both a documentation signal (docs words in its host, path or title) and
/// the topic's own words (in its host or title); known tutorial and Q&A
/// sites never qualify. Ties keep Google's order.
pub fn pick_official(topic: &str, results: &[SearchResult]) -> Option<String> {
    let words = topic_words(topic);
    results
        .iter()
        .enumerate()
        .filter_map(|(rank, result)| {
            let url = Url::parse(&result.link).ok()?;
            let host = url.host_str()?.trim_start_matches("www.").to_lowercase();
            if NOT_OFFICIAL.iter().any(|bad| host == *bad || host.ends_with(&format!(".{bad}"))) {
                return None;
            }
            let path = url.path().to_lowercase();
            let title = result.title.to_lowercase();
            let host_words = normalized(&host);
            let in_host = words.iter().filter(|w| host_words.contains(w.as_str())).count() as i32;
            let in_title = words.iter().filter(|w| title.contains(w.as_str())).count() as i32;
            let docs_host = DOC_WORDS.iter().any(|w| host.split('.').any(|part| part == *w));
            let docs_path = DOC_WORDS
                .iter()
                .any(|w| path.split(['/', '-', '.']).any(|part| part == *w));
            let docs_title = title.contains("documentation") || title.contains(" docs");
            if !(docs_host || docs_path || docs_title) || in_host + in_title == 0 {
                return None;
            }
            let mut score = 4 * in_host + 2 * in_title;
            score += if docs_host { 3 } else { 0 } + if docs_path { 2 } else { 0 };
            score += i32::from(docs_title);
            if host == "github.com" {
                score -= 2;
            }
            Some((score, std::cmp::Reverse(rank), result.link.clone()))
        })
        .max()
        .map(|(_, _, link)| link)
}

/// What Gemini is given about the docs.
pub struct DocsContext {
    /// Every page that was read, landing page first.
    pub pages: Vec<String>,
    pub text: String,
}

/// Reads the landing page and up to two "getting started" pages on the same
/// site, and combines their outlines with an excerpt of the landing page.
pub fn gather_context(agent: &ureq::Agent, url: &str, topic: &str) -> Result<DocsContext> {
    let html = fetch_html(agent, url)?;
    let base = Url::parse(url).with_context(|| format!("not a URL: {url}"))?;
    let landing = Html::parse_document(&html);
    if !is_relevant(topic, &document_text(&landing)) {
        bail!("{url} doesn't look like documentation for {topic}");
    }

    let mut text = format!("Official documentation: {url}\n\nOutline of {url}:\n");
    text.push_str(&outline(&landing).join("\n"));

    let mut pages = vec![url.to_string()];
    for link in start_links(&landing, &base).into_iter().take(MAX_FOLLOW_UPS) {
        let Ok(page) = fetch_html(agent, link.as_str()) else {
            continue;
        };
        let lines = outline(&Html::parse_document(&page));
        if !lines.is_empty() {
            text.push_str(&format!("\n\nOutline of {link}:\n{}", lines.join("\n")));
            pages.push(link.to_string());
        }
    }

    let excerpt = extract_excerpt(&document_text(&landing));
    if !excerpt.is_empty() {
        text.push_str("\n\nExcerpt from the landing page:\n");
        text.push_str(&excerpt);
    }
    Ok(DocsContext {
        pages,
        text: take_chars(&text, MAX_CONTEXT).to_string(),
    })
}

fn fetch_html(agent: &ureq::Agent, url: &str) -> Result<String> {
    let mut response = agent.get(url).call().with_context(|| format!("fetching {url}"))?;
    let status = response.status();
    if !status.is_success() {
        bail!("{url} returned HTTP {status}");
    }
    response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("reading {url}"))
}

/// The page's h1-h3 headings as an indented list, deduplicated.
pub fn outline(document: &Html) -> Vec<String> {
    let headings = Selector::parse("h1, h2, h3").expect("static selector");
    let mut seen = std::collections::HashSet::new();
    document
        .select(&headings)
        .filter_map(|heading| {
            let text = heading.text().collect::<Vec<_>>().join(" ");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = text.trim_end_matches(['#', '¶']).trim().to_string();
            if text.is_empty() || text.len() > 120 || !seen.insert(text.clone()) {
                return None;
            }
            let indent = match heading.value().name() {
                "h1" => "",
                "h2" => "  ",
                _ => "    ",
            };
            Some(format!("{indent}- {text}"))
        })
        .take(MAX_OUTLINE)
        .collect()
}

/// Same-site links whose text reads like an entry point into the docs.
fn start_links(document: &Html, base: &Url) -> Vec<Url> {
    let anchors = Selector::parse("a[href]").expect("static selector");
    let mut found: Vec<Url> = Vec::new();
    for anchor in document.select(&anchors) {
        let label = anchor.text().collect::<String>().to_lowercase();
        let label = label.trim();
        if label.is_empty() || label.len() > 40 || !START_WORDS.iter().any(|w| label.contains(w)) {
            continue;
        }
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(mut link) = base.join(href) else {
            continue;
        };
        link.set_fragment(None);
        if link.host_str() == base.host_str() && link != *base && !found.contains(&link) {
            found.push(link);
        }
    }
    found
}

#[cfg(test)]
fn html_to_text(html: &str) -> String {
    document_text(&Html::parse_document(html))
}

fn document_text(document: &Html) -> String {
    let body = Selector::parse("body").expect("static selector");
    let mut raw = String::new();
    match document.select(&body).next() {
        Some(body) => collect_text(body, &mut raw),
        None => collect_text(document.root_element(), &mut raw),
    }
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_text(element: ElementRef, out: &mut String) {
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            out.push_str(text);
            out.push(' ');
        } else if let Some(child) = ElementRef::wrap(child)
            && !SKIPPED_ELEMENTS.contains(&child.value().name())
        {
            collect_text(child, out);
        }
    }
}

/// ~500 characters after each well-known section heading, or the first 2000
/// characters when none appear. Mirrors the web app's `processDocumentationData`.
pub fn extract_excerpt(text: &str) -> String {
    let mut excerpt = String::new();
    for section in SECTIONS {
        if let Some(start) = text.find(section) {
            excerpt.push_str(take_chars(&text[start..], SECTION_WINDOW));
            excerpt.push_str("\n\n");
        }
    }
    if excerpt.is_empty() {
        excerpt.push_str(take_chars(text, FALLBACK_LEN));
    }
    excerpt.trim_end().to_string()
}

/// The first `n` characters of `text`, never splitting a UTF-8 sequence.
fn take_chars(text: &str, n: usize) -> &str {
    match text.char_indices().nth(n) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(link: &str, title: &str) -> SearchResult {
        SearchResult {
            link: link.into(),
            title: title.into(),
        }
    }

    #[test]
    fn known_urls_ignore_case_spaces_and_punctuation() {
        assert_eq!(known_docs_url("Next.js"), Some("https://nextjs.org/docs"));
        assert_eq!(known_docs_url(" React "), Some("https://react.dev/learn"));
        assert_eq!(known_docs_url("Rust async"), Some("https://rust-lang.github.io/async-book/"));
        assert_eq!(known_docs_url("Linear algebra"), None);
    }

    #[test]
    fn picks_the_official_site_over_mirrors_and_tutorials() {
        let results = [
            result("https://rust.docs.kernel.org/core/keyword.async.html", "async - Rust"),
            result("https://www.geeksforgeeks.org/rust-async/", "Rust async tutorial"),
            result("https://rust-lang.github.io/async-book/", "Asynchronous Programming in Rust"),
            result("https://doc.rust-lang.org/std/keyword.async.html", "async - Rust documentation"),
        ];
        let picked = pick_official("Rust async", &results).unwrap();
        assert!(picked.contains("rust-lang"), "picked {picked}");
    }

    #[test]
    fn product_sites_that_share_a_word_are_not_docs() {
        let results = [
            result("https://linear.app/", "Linear - Plan and build products"),
            result("https://www.geeksforgeeks.org/linear-algebra/", "Linear algebra"),
        ];
        assert_eq!(pick_official("Linear algebra for ML", &results), None);
        assert!(!is_relevant("Linear algebra for ML", "Linear helps teams plan and ship products"));
        assert!(is_relevant("Linear algebra for ML", "Matrices and vectors in linear algebra"));
        assert_eq!(topic_words("Linear algebra for ML"), ["linear", "algebra"]);
    }

    #[test]
    fn nothing_official_means_no_pick() {
        let results = [result("https://medium.com/@x/rust", "Rust"), result("not a url", "")];
        assert_eq!(pick_official("Rust", &results), None);
    }

    #[test]
    fn outline_indents_and_dedupes_headings() {
        let html = Html::parse_document(
            "<body><h1>Book</h1><h2>Getting  Started ¶</h2><h3>Install</h3><h2>Getting Started</h2></body>",
        );
        assert_eq!(outline(&html), ["- Book", "  - Getting Started", "    - Install"]);
    }

    #[test]
    fn start_links_stay_on_site_and_resolve_relative_hrefs() {
        let base = Url::parse("https://example.dev/docs/").unwrap();
        let html = Html::parse_document(
            r##"<body><a href="getting-started#top">Getting started</a>
               <a href="https://other.com/learn">Learn</a><a href="/api">API</a></body>"##,
        );
        let links: Vec<String> = start_links(&html, &base).iter().map(Url::to_string).collect();
        assert_eq!(links, ["https://example.dev/docs/getting-started"]);
    }

    #[test]
    fn html_text_skips_scripts_and_collapses_whitespace() {
        let html = r#"<html><head><title>T</title></head><body>
            <nav>Docs</nav><script>var secret = 1;</script>
            <h1>Getting   Started</h1><style>.x{}</style><p>Install it.</p></body></html>"#;
        assert_eq!(html_to_text(html), "Docs Getting Started Install it.");
    }

    #[test]
    fn excerpt_falls_back_to_the_start_and_respects_char_boundaries() {
        let text = "é".repeat(3000);
        assert_eq!(extract_excerpt(&text).chars().count(), FALLBACK_LEN);
    }
}
