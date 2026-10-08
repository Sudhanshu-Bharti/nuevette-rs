//! The two-step generation: a quick outline the learner reviews, then the
//! full path streamed from Gemini so topics can appear as they are written.

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::gemini::{
    self, ENDPOINT, GeminiError, Intent, Learner, api_error, candidate_parts, finish_path, is_transient,
    path_schema,
};
use super::docs::NavEntry;
use crate::model::{LearningPath, Subtopic};

/// A draft the learner can edit before the full path is built.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub name: String,
    pub description: String,
    pub topics: Vec<OutlineTopic>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutlineTopic {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The docs pages this topic covers, in order, when the outline was built
    /// from the docs' table of contents; each becomes one step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<OutlineStep>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutlineStep {
    pub title: String,
    pub url: String,
}

/// The outline as Gemini returns it: topics refer to table-of-contents
/// entries by number, so it can only choose steps, never invent them.
#[derive(Deserialize)]
struct RawOutline {
    name: String,
    description: String,
    topics: Vec<RawTopic>,
}

#[derive(Deserialize)]
struct RawTopic {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    entries: Vec<i64>,
}

/// Turns entry numbers into steps. Unknown numbers, and entries already used
/// by an earlier topic, are dropped.
fn resolve(raw: RawOutline, nav: &[NavEntry]) -> Outline {
    let mut used = std::collections::HashSet::new();
    Outline {
        name: raw.name,
        description: raw.description,
        topics: raw
            .topics
            .into_iter()
            .map(|topic| OutlineTopic {
                steps: topic
                    .entries
                    .iter()
                    .filter_map(|n| usize::try_from(*n).ok()?.checked_sub(1))
                    .filter(|ix| *ix < nav.len() && used.insert(*ix))
                    .map(|ix| OutlineStep { title: nav[ix].title.clone(), url: nav[ix].url.clone() })
                    .collect(),
                name: topic.name,
                description: topic.description,
            })
            .filter(|topic| nav.is_empty() || !topic.steps.is_empty())
            .collect(),
    }
}

fn outline_schema(grounded: bool) -> Value {
    let string = json!({ "type": "STRING" });
    let topic = if grounded {
        json!({
            "type": "OBJECT",
            "properties": {
                "name": string, "description": string,
                "entries": { "type": "ARRAY", "items": { "type": "INTEGER" } },
            },
            "required": ["name", "description", "entries"],
            "propertyOrdering": ["name", "description", "entries"],
        })
    } else {
        json!({
            "type": "OBJECT",
            "properties": { "name": string, "description": string },
            "required": ["name", "description"],
            "propertyOrdering": ["name", "description"],
        })
    };
    json!({
        "type": "OBJECT",
        "properties": {
            "name": string, "description": string,
            "topics": { "type": "ARRAY", "items": topic },
        },
        "required": ["name", "description", "topics"],
        "propertyOrdering": ["name", "description", "topics"],
    })
}

pub fn outline_prompt(
    topic: &str,
    learner: Learner,
    intent: &Intent,
    docs_context: &str,
    nav: &[NavEntry],
) -> String {
    let base = gemini::build_prompt(topic, learner, intent, docs_context);
    let base = base.split("\n\nOrganize it as").next().unwrap_or_default();
    if nav.is_empty() {
        return format!(
            "{base}\n\nFor now, plan only: give the path a short name, a one-sentence description, \
             and its 5-7 topics (each a name and a one-sentence description), ordered from \
             foundational to advanced. Do not list subtopics yet."
        );
    }
    let contents: Vec<String> = nav
        .iter()
        .enumerate()
        .map(|(ix, entry)| format!("{}{}. {}", "  ".repeat(entry.depth), ix + 1, entry.title))
        .collect();
    format!(
        "{base}\n\nThis is the documentation's own table of contents, numbered:\n{}\n\n\
         Plan the path from these entries only. Give it a short name, a one-sentence description, \
         and 5-7 topics ordered from foundational to advanced, each with a name of your own, a \
         one-sentence description, and `entries`: the numbers of 3-8 entries it covers, in the \
         order to study them. Use each number at most once. Leave out entries that don't help \
         this learner (changelogs, legal pages, API listings a beginner doesn't need). Never use \
         a number that isn't in the list.",
        contents.join("\n"),
    )
}

/// The full prompt, with the reviewed outline fixed in place.
pub fn build_prompt(
    topic: &str,
    learner: Learner,
    intent: &Intent,
    docs_context: &str,
    outline: &Outline,
) -> String {
    let topics: Vec<String> = outline
        .topics
        .iter()
        .enumerate()
        .map(|(ix, t)| {
            let mut line = if t.description.trim().is_empty() {
                format!("{}. {}", ix + 1, t.name)
            } else {
                format!("{}. {}: {}", ix + 1, t.name, t.description)
            };
            if !t.steps.is_empty() {
                line.push_str(&format!(
                    "\n   Write exactly {} subtopics for this topic, one per documentation page, in \
                     this order, each named after its page:",
                    t.steps.len()
                ));
                for step in &t.steps {
                    line.push_str(&format!("\n   - {} ({})", step.title, step.url));
                }
            }
            line
        })
        .collect();
    format!(
        "{}\n\nThe learner approved this outline. Name the path \"{}\" and use exactly these \
         topics, in this order, keeping their names:\n{}",
        gemini::build_prompt(topic, learner, intent, docs_context),
        outline.name,
        topics.join("\n"),
    )
}

pub fn draft_outline(
    agent: &ureq::Agent,
    api_key: &str,
    model: &str,
    prompt: &str,
    nav: &[NavEntry],
    cancelled: &AtomicBool,
) -> Result<Outline, GeminiError> {
    let raw: RawOutline =
        gemini::generate(agent, api_key, model, prompt, outline_schema(!nav.is_empty()), cancelled)?;
    let outline = resolve(raw, nav);
    if outline.topics.is_empty() {
        return Err(anyhow!("Gemini returned an outline with no topics").into());
    }
    Ok(outline)
}

/// Streams the full path, calling `on_text` with each fragment of JSON as it
/// arrives, and returns the whole text. Retries 5xx before the stream starts.
pub fn stream(
    agent: &ureq::Agent,
    api_key: &str,
    model: &str,
    prompt: &str,
    cancelled: &AtomicBool,
    on_text: &mut dyn FnMut(&str),
) -> Result<String, GeminiError> {
    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": path_schema(),
            "temperature": 0.4,
        },
    });
    let url = format!("{ENDPOINT}/{model}:streamGenerateContent?alt=sse");
    let mut attempt = 1;
    let response = loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GeminiError::Failed(anyhow!("cancelled")));
        }
        let mut response = match agent.post(&url).header("x-goog-api-key", api_key).send_json(&body) {
            Ok(response) => response,
            Err(ureq::Error::Timeout(_)) => {
                return Err(GeminiError::Unavailable(anyhow!("{model} didn't answer in time")));
            }
            Err(error) => {
                let message = super::redact(format!("contacting Gemini: {error}"), api_key);
                return Err(GeminiError::Failed(anyhow!(message)));
            }
        };
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            break response;
        }
        let text = response.body_mut().read_to_string().unwrap_or_default();
        if is_transient(status) && attempt < 3 && !cancelled.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(1 << attempt));
            attempt += 1;
            continue;
        }
        let error = api_error(status, &text, model);
        return Err(if is_transient(status) || status == 429 {
            GeminiError::Unavailable(error)
        } else {
            GeminiError::Failed(error)
        });
    };

    let mut full = String::new();
    let reader = BufReader::new(response.into_body().into_reader());
    for line in reader.lines() {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GeminiError::Failed(anyhow!("cancelled")));
        }
        let line = line.context("reading Gemini's stream")?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let (text, finish) = candidate_parts(data.trim())?;
        if finish.as_deref() == Some("MAX_TOKENS") {
            return Err(anyhow!("Gemini's answer was cut off before it was complete").into());
        }
        if !text.is_empty() {
            on_text(&text);
            full.push_str(&text);
        }
    }
    Ok(full)
}

/// The finished path from the streamed text, with the outline's topic names
/// kept exactly as the learner approved them.
pub fn finish(text: &str, learner: Learner, outline: &Outline) -> Result<LearningPath> {
    let mut path: LearningPath = gemini::parse_json(text)?;
    if path.topics.len() != outline.topics.len() {
        bail!(
            "Gemini returned {} topics instead of the {} in the outline",
            path.topics.len(),
            outline.topics.len()
        );
    }
    for (topic, planned) in path.topics.iter_mut().zip(&outline.topics) {
        topic.name = planned.name.clone();
        cite(&mut topic.subtopics, &planned.steps);
    }
    path.name = outline.name.clone();
    finish_path(&mut path, learner)?;
    Ok(path)
}

/// Ties a topic's steps to the docs pages they were planned from: one step
/// per page, in order, each citing its page (missing ones get a stub so the
/// page is never lost; extras the model added are dropped).
fn cite(subtopics: &mut Vec<Subtopic>, steps: &[OutlineStep]) {
    if steps.is_empty() {
        return;
    }
    subtopics.truncate(steps.len());
    for (ix, step) in steps.iter().enumerate() {
        if ix == subtopics.len() {
            subtopics.push(Subtopic {
                name: step.title.clone(),
                description: String::new(),
                estimated_time: String::new(),
                estimated_hours: None,
                technologies_and_concepts: Vec::new(),
                prerequisites: Vec::new(),
                resources: Vec::new(),
                source: None,
                importance: Default::default(),
            });
        }
        let sub = &mut subtopics[ix];
        sub.source = Some(step.url.clone());
        sub.resources.retain(|r| r != &step.url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Level;

    const LEARNER: Learner = Learner {
        level: Level::Beginner,
        hours_per_week: None,
    };

    fn outline() -> Outline {
        Outline {
            name: "My Rust".into(),
            description: "d".into(),
            topics: vec![
                OutlineTopic { name: "Basics".into(), description: "start here".into(), steps: Vec::new() },
                OutlineTopic { name: "Ownership".into(), description: String::new(), steps: Vec::new() },
            ],
        }
    }

    #[test]
    fn prompts_plan_first_then_lock_the_outline() {
        let plan = outline_prompt("Rust", LEARNER, &Intent::default(), "", &[]);
        assert!(plan.contains("plan only") && !plan.contains("Organize it as"));
        let build = build_prompt("Rust", LEARNER, &Intent::default(), "", &outline());
        assert!(build.contains("Name the path \"My Rust\""));
        assert!(build.contains("1. Basics: start here\n2. Ownership"));
    }

    #[test]
    fn finishing_keeps_the_approved_names() {
        let text = r#"{"name":"Other","description":"d","topics":[
            {"name":"Basics!","description":"d","subtopics":[{"name":"a","description":"d","estimatedHours":2,
              "technologiesAndConcepts":[],"prerequisites":[],"resources":[]}]},
            {"name":"Owner","description":"d","subtopics":[{"name":"b","description":"d","estimatedHours":1,
              "technologiesAndConcepts":[],"prerequisites":[],"resources":[]}]}]}"#;
        let path = finish(text, LEARNER, &outline()).unwrap();
        assert_eq!(path.name, "My Rust");
        assert_eq!(path.topics[1].name, "Ownership");
        assert_eq!(path.estimated_time, "~3 hours");

        let short = r#"{"name":"x","description":"d","topics":[{"name":"a","description":"d","subtopics":[]}]}"#;
        assert!(finish(short, LEARNER, &outline()).unwrap_err().to_string().contains("instead of the 2"));
    }
}
