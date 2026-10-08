//! Gemini `generateContent` with structured JSON output. Response schemas
//! mirror `model::LearningPath` / `model::Topic`, so replies deserialize
//! directly; hours come back as numbers and totals are computed locally.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::model::{LearningPath, Level, Topic, new_path_id};

pub(super) const ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/models";
/// Attempts for transient server errors (5xx), e.g. 503 "model overloaded".
const MAX_ATTEMPTS: u32 = 3;

/// Who the path is for.
#[derive(Clone, Copy, Debug)]
pub struct Learner {
    pub level: Level,
    /// Hours per week, when the learner set a pace.
    pub hours_per_week: Option<u32>,
}

/// What the learner told us beyond the topic.
#[derive(Clone, Debug, Default)]
pub struct Intent {
    /// "I want to be able to ..."
    pub goal: Option<String>,
    /// What they already know.
    pub background: Option<String>,
}

impl Intent {
    fn brief(&self) -> String {
        let mut out = String::new();
        if let Some(goal) = &self.goal {
            out.push_str(&format!(
                " Their goal: {goal}. Shape the path toward it and end with a topic that applies it."
            ));
        }
        if let Some(background) = &self.background {
            out.push_str(&format!(" They already know: {background}. Don't re-teach it."));
        }
        out
    }
}

impl Learner {
    fn brief(&self, topic: &str) -> String {
        let level = match self.level {
            Level::Beginner => format!(
                "a beginner with no prior experience of {topic}; start from the fundamentals"
            ),
            Level::Intermediate => format!(
                "an intermediate learner who knows the basics of {topic}; skip introductions and \
                 focus on depth, idiomatic practice and real projects"
            ),
            Level::Advanced => format!(
                "an advanced practitioner of {topic}; skip fundamentals and focus on internals, \
                 performance, edge cases and expert techniques"
            ),
        };
        let pace = self.hours_per_week.map_or(String::new(), |hours| {
            format!(
                " They can study about {hours} hours per week: keep the total realistic and \
                 say in the path description roughly how many weeks it takes."
            )
        });
        format!("The learner is {level}.{pace}")
    }
}

const RULES: &str = "For each subtopic give: a one- or two-sentence description of what it covers, \
     citing the documentation where relevant; estimatedHours, a realistic number of hours for \
     this learner (decimals allowed, e.g. 1.5); short labels for the technologies, tools or \
     concepts to study; prerequisites, naming earlier subtopics where they apply; and 1-3 \
     resources as full https:// URLs to specific pages of the official documentation or other \
     well-known authoritative sites. Only give URLs you are confident exist; never invent them. \
     Mark each subtopic's importance: \"core\" for what every learner needs, \"optional\" for \
     useful extras that can be skipped, \"alternative\" when it is one of several ways to do \
     the same thing. Most subtopics are core. \
     Do not include placeholder text or comments like \"other topics would follow\".";

fn docs_block(docs_context: &str) -> String {
    if docs_context.trim().is_empty() {
        "No documentation was available. Rely on your knowledge of the official documentation."
            .into()
    } else {
        format!(
            "Use the structure of the official documentation below: follow its order and \
             reference its sections. If it turns out not to be about this topic, ignore it.\n\n\
             {docs_context}"
        )
    }
}

pub fn build_prompt(topic: &str, learner: Learner, intent: &Intent, docs_context: &str) -> String {
    format!(
        "Create a learning path for {topic}. {}{}\n\n{}\n\n\
         Organize it as 5-7 topics, ordered from foundational to advanced, each with 3-5 \
         subtopics. Give the path a short name and a one-sentence description, and each topic \
         a one-sentence description. {RULES}",
        learner.brief(topic),
        intent.brief(),
        docs_block(docs_context),
    )
}

pub fn build_expand_prompt(path: &LearningPath, topic_ix: usize, learner: Learner) -> String {
    let topic = &path.topics[topic_ix];
    let outline: Vec<String> = path
        .topics
        .iter()
        .enumerate()
        .map(|(ix, t)| format!("{}. {}", ix + 1, t.name))
        .collect();
    let current: Vec<String> = topic.subtopics.iter().map(|s| format!("- {}", s.name)).collect();
    format!(
        "This is a learning path for {name}. {brief}\n\nIts topics:\n{outline}\n\n\
         Expand topic {number}, \"{title}\" ({description}), in more depth. It currently has:\n\
         {current}\n\nReturn this topic with 5-7 subtopics that go deeper and are more concrete, \
         keeping its name. Do not repeat material that belongs to the other topics. {RULES}",
        name = path.name,
        brief = learner.brief(&path.name),
        outline = outline.join("\n"),
        number = topic_ix + 1,
        title = topic.name,
        description = topic.description,
        current = current.join("\n"),
    )
}

fn topic_schema() -> Value {
    let string = json!({ "type": "STRING" });
    let strings = json!({ "type": "ARRAY", "items": { "type": "STRING" } });
    let subtopic = json!({
        "type": "OBJECT",
        "properties": {
            "name": string, "description": string, "estimatedHours": { "type": "NUMBER" },
            "technologiesAndConcepts": strings, "prerequisites": strings, "resources": strings,
            "importance": { "type": "STRING", "enum": ["core", "optional", "alternative"] },
        },
        "required": ["name", "description", "estimatedHours", "technologiesAndConcepts",
                     "prerequisites", "resources", "importance"],
        "propertyOrdering": ["name", "description", "estimatedHours", "technologiesAndConcepts",
                             "prerequisites", "resources", "importance"],
    });
    json!({
        "type": "OBJECT",
        "properties": {
            "name": string, "description": string,
            "subtopics": { "type": "ARRAY", "items": subtopic },
        },
        "required": ["name", "description", "subtopics"],
        "propertyOrdering": ["name", "description", "subtopics"],
    })
}

/// OpenAPI-subset schema accepted by `generationConfig.responseSchema`.
pub fn path_schema() -> Value {
    let string = json!({ "type": "STRING" });
    json!({
        "type": "OBJECT",
        "properties": {
            "name": string, "description": string,
            "topics": { "type": "ARRAY", "items": topic_schema() },
        },
        "required": ["name", "description", "topics"],
        "propertyOrdering": ["name", "description", "topics"],
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    prompt_feedback: Option<PromptFeedback>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<Content>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Content {
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    text: Option<String>,
    #[serde(default)]
    thought: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    block_reason: Option<String>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    status: String,
}

/// Why a model couldn't produce an answer.
#[derive(Debug)]
pub enum GeminiError {
    /// Overloaded or timed out even after retries; another model may work.
    Unavailable(anyhow::Error),
    Failed(anyhow::Error),
}

impl GeminiError {
    pub fn into_inner(self) -> anyhow::Error {
        match self {
            Self::Unavailable(error) | Self::Failed(error) => error,
        }
    }
}

impl From<anyhow::Error> for GeminiError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

/// One Gemini call with structured output and retries for 5xx. Stops early,
/// without retrying, once `cancelled` is set.
pub(super) fn generate<T: DeserializeOwned>(
    agent: &ureq::Agent,
    api_key: &str,
    model: &str,
    prompt: &str,
    schema: Value,
    cancelled: &AtomicBool,
) -> Result<T, GeminiError> {
    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": schema,
            "temperature": 0.4,
        },
    });
    let url = format!("{ENDPOINT}/{model}:generateContent");
    let mut attempt = 1;
    loop {
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
        let text = response
            .body_mut()
            .read_to_string()
            .context("reading Gemini's response")?;
        if (200..300).contains(&status) {
            return Ok(parse_json(&candidate_text(&text)?)?);
        }
        if is_transient(status) && attempt < MAX_ATTEMPTS && !cancelled.load(Ordering::Relaxed) {
            // Runs on a background thread, so blocking here is fine: 2s, then 4s.
            std::thread::sleep(std::time::Duration::from_secs(1 << attempt));
            attempt += 1;
            continue;
        }
        let error = api_error(status, &text, model);
        // Quotas are per model, so a 429 is worth trying on the next model too.
        return Err(if is_transient(status) || status == 429 {
            GeminiError::Unavailable(error)
        } else {
            GeminiError::Failed(error)
        });
    }
}

/// A deeper replacement for one topic of `path`, with its original name.
pub fn expand_topic(
    agent: &ureq::Agent,
    api_key: &str,
    model: &str,
    path: &LearningPath,
    topic_ix: usize,
    learner: Learner,
    cancelled: &AtomicBool,
) -> Result<Topic, GeminiError> {
    let prompt = build_expand_prompt(path, topic_ix, learner);
    let mut topic: Topic = generate(agent, api_key, model, &prompt, topic_schema(), cancelled)?;
    if topic.subtopics.is_empty() {
        return Err(anyhow!("Gemini returned the topic without subtopics").into());
    }
    topic.name = path.topics[topic_ix].name.clone();
    Ok(topic)
}

pub(super) fn finish_path(path: &mut LearningPath, learner: Learner) -> Result<()> {
    if path.topics.is_empty() {
        bail!("Gemini returned a learning path with no topics");
    }
    path.id = new_path_id();
    path.level = Some(learner.level);
    path.normalize_times();
    Ok(())
}

pub(super) fn is_transient(status: u16) -> bool {
    matches!(status, 500 | 502 | 503 | 504)
}

pub(super) fn api_error(status: u16, body: &str, model: &str) -> anyhow::Error {
    let detail = serde_json::from_str::<ErrorBody>(body)
        .map(|b| match (b.error.message.trim(), b.error.status.trim()) {
            ("", "") => format!("HTTP {status}"),
            (message, "") => message.to_string(),
            ("", code) => code.to_string(),
            (message, code) => format!("{message} [{code}]"),
        })
        .unwrap_or_else(|_| format!("HTTP {status}"));
    match status {
        400 | 401 | 403 if detail.to_lowercase().contains("api key") => {
            anyhow!("Gemini rejected the API key ({detail}). Check GEMINI_API_KEY in .env.")
        }
        404 => anyhow!("Gemini model \"{model}\" isn't available ({detail}). Set GEMINI_MODEL in .env."),
        429 => quota_error(body, model),
        _ if is_transient(status) => anyhow!(
            "{model} is overloaded right now (HTTP {status}, still failing after {MAX_ATTEMPTS} \
             attempts)"
        ),
        _ => anyhow!("Gemini returned HTTP {status}: {detail}"),
    }
}

/// "429 RESOURCE_EXHAUSTED" in one readable sentence: which model, the
/// limit when Google states it, and when it resets.
fn quota_error(body: &str, model: &str) -> anyhow::Error {
    let value: Value = serde_json::from_str(body).unwrap_or_default();
    let message = value["error"]["message"].as_str().unwrap_or_default();
    let limit = message
        .split("limit: ")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .filter(|digits| !digits.is_empty());
    let retry_secs = value["error"]["details"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|detail| detail["retryDelay"].as_str())
        .and_then(|delay| delay.trim_end_matches('s').parse::<f64>().ok());
    let daily = message.contains("free_tier") || message.contains("per day");
    let what = match (limit, daily) {
        (Some(limit), true) => format!("{model} has used its free-tier quota ({limit} requests a day)"),
        _ => format!("{model} has hit its rate limit"),
    };
    let when = match retry_secs {
        Some(secs) if secs >= 3600. => {
            format!("It resets in about {} h {} min", (secs / 3600.) as u64, ((secs % 3600.) / 60.) as u64)
        }
        Some(secs) if secs >= 60. => format!("It resets in about {} min", (secs / 60.).ceil() as u64),
        Some(_) => "Try again in a few seconds".into(),
        None => "Try again later".into(),
    };
    let billing = if daily { ", or enable billing in Google AI Studio for higher limits" } else { "" };
    anyhow!("{what}. {when}{billing}.")
}

/// The model's answer text from a `generateContent` response body.
fn candidate_text(body: &str) -> Result<String> {
    let (text, finish) = candidate_parts(body)?;
    if finish.as_deref() == Some("MAX_TOKENS") {
        bail!("Gemini's answer was cut off before it was complete");
    }
    Ok(text)
}

/// The answer text and finish reason from one response, or one streamed
/// chunk of a response (where the text is a fragment).
pub(super) fn candidate_parts(body: &str) -> Result<(String, Option<String>)> {
    let response: GenerateResponse =
        serde_json::from_str(body).context("unexpected response shape from Gemini")?;
    let Some(candidate) = response.candidates.into_iter().next() else {
        let reason = response
            .prompt_feedback
            .and_then(|f| f.block_reason)
            .unwrap_or_else(|| "no candidates returned".into());
        bail!("Gemini declined the request ({reason})");
    };
    let text = candidate
        .content
        .map(|c| c.parts)
        .unwrap_or_default()
        .into_iter()
        .filter(|part| !part.thought)
        .filter_map(|part| part.text)
        .collect();
    Ok((text, candidate.finish_reason))
}

/// Parses JSON text, stripping code fences defensively as the web app did.
pub(super) fn parse_json<T: DeserializeOwned>(text: &str) -> Result<T> {
    let json = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(json).context("Gemini's answer didn't match the expected shape")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH_JSON: &str = r#"{"name":"Rust async","description":"d","topics":[
        {"name":"Futures","description":"d","subtopics":[
          {"name":"Poll","description":"d","estimatedHours":1.5,
           "technologiesAndConcepts":["Future"],"prerequisites":[],"resources":["https://x.dev"]},
          {"name":"Wake","description":"d","estimatedHours":2,
           "technologiesAndConcepts":[],"prerequisites":["Poll"],"resources":[]}]}]}"#;

    const LEARNER: Learner = Learner {
        level: Level::Intermediate,
        hours_per_week: Some(5),
    };

    fn response(text: &str) -> String {
        json!({
            "candidates": [{
                "content": { "parts": [{ "text": "thinking...", "thought": true }, { "text": text }] },
                "finishReason": "STOP"
            }]
        })
        .to_string()
    }

    #[test]
    fn parses_path_skips_thoughts_and_computes_totals() {
        let text = candidate_text(&response(PATH_JSON)).unwrap();
        let mut path: LearningPath = parse_json(&text).unwrap();
        finish_path(&mut path, LEARNER).unwrap();
        assert_eq!(path.topics[0].subtopics[0].estimated_time, "~1.5 hours");
        assert_eq!(path.topics[0].estimated_time, "~3.5 hours");
        assert_eq!(path.estimated_time, "~3.5 hours");
        assert_eq!(path.level, Some(Level::Intermediate));
        assert!(!path.id.is_empty());
    }

    #[test]
    fn strips_code_fences_and_rejects_empty_paths() {
        let fenced = format!("```json\n{PATH_JSON}\n```");
        assert!(parse_json::<LearningPath>(&fenced).is_ok());
        let mut empty: LearningPath =
            parse_json(r#"{"name":"x","description":"d","topics":[]}"#).unwrap();
        assert!(finish_path(&mut empty, LEARNER).is_err());
    }

    #[test]
    fn blocked_and_truncated_responses_are_errors() {
        let blocked = json!({ "promptFeedback": { "blockReason": "SAFETY" } }).to_string();
        assert!(candidate_text(&blocked).unwrap_err().to_string().contains("SAFETY"));
        let truncated = json!({ "candidates": [{ "content": { "parts": [{ "text": "{" }] },
                                                  "finishReason": "MAX_TOKENS" }] })
        .to_string();
        assert!(candidate_text(&truncated).unwrap_err().to_string().contains("cut off"));
    }

    #[test]
    fn api_errors_name_the_setting_to_fix() {
        let body = r#"{"error":{"code":404,"message":"models/nope is not found","status":"NOT_FOUND"}}"#;
        let message = api_error(404, body, "nope").to_string();
        assert!(message.contains("GEMINI_MODEL") && message.contains("not found"));
        let body = r#"{"error":{"message":"API key not valid.","status":"INVALID_ARGUMENT"}}"#;
        assert!(api_error(400, body, "m").to_string().contains("GEMINI_API_KEY"));
        let quota = r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED",
            "message":"You exceeded your current quota. * Quota exceeded for metric: generativelanguage.googleapis.com/generate_content_free_tier_requests, limit: 20, model: gemini-3.8-flash",
            "details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"35873s"}]}}"#;
        assert_eq!(
            api_error(429, quota, "gemini-3.8-flash").to_string(),
            "gemini-3.8-flash has used its free-tier quota (20 requests a day). It resets in \
             about 9 h 57 min, or enable billing in Google AI Studio for higher limits."
        );
        let message = api_error(503, "{}", "m").to_string();
        assert!(message.contains("overloaded") && message.contains("3 attempts"));
        assert!(is_transient(503) && !is_transient(429) && !is_transient(400));
    }

    #[test]
    fn prompts_carry_level_pace_and_docs() {
        let intent = Intent {
            goal: Some("ship a CLI".into()),
            background: Some("Python".into()),
        };
        let prompt = build_prompt("Rust", LEARNER, &intent, "Outline of https://x.dev:\n- Book");
        assert!(prompt.contains("intermediate") && prompt.contains("5 hours per week"));
        assert!(prompt.contains("- Book"));
        assert!(prompt.contains("Their goal: ship a CLI") && prompt.contains("already know: Python"));
        assert!(build_prompt("Rust", LEARNER, &Intent::default(), "").contains("No documentation"));

        let path: LearningPath = parse_json(PATH_JSON).unwrap();
        let expand = build_expand_prompt(&path, 0, LEARNER);
        assert!(expand.contains("Expand topic 1, \"Futures\"") && expand.contains("- Wake"));
    }

    #[test]
    fn schema_requires_numeric_hours() {
        let schema = path_schema();
        let subtopic = &schema["properties"]["topics"]["items"]["properties"]["subtopics"]["items"];
        assert_eq!(subtopic["properties"]["estimatedHours"]["type"], "NUMBER");
        assert_eq!(subtopic["required"].as_array().unwrap().len(), 7);
    }
}
