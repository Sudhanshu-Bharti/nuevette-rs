//! Learning-path domain types. Field names serialize as camelCase so the same
//! structs read the bundled samples, the persisted store, and Gemini's JSON.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subtopic {
    pub name: String,
    pub description: String,
    /// Display form of the estimate, e.g. "~2 hours". Derived from
    /// `estimated_hours` when that is known.
    #[serde(default)]
    pub estimated_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_hours: Option<f32>,
    #[serde(default)]
    pub technologies_and_concepts: Vec<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub estimated_time: String,
    #[serde(default)]
    pub subtopics: Vec<Subtopic>,
}

impl Topic {
    /// Fills display times from numeric subtopic hours; a missing estimate
    /// counts as one hour.
    pub fn with_display_times(mut self) -> Self {
        let mut total = 0.;
        for sub in &mut self.subtopics {
            let hours = sub.estimated_hours.unwrap_or(1.).max(0.);
            sub.estimated_time = format_hours(hours);
            total += hours;
        }
        if !self.subtopics.is_empty() {
            self.estimated_time = format_hours(total);
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    #[default]
    Beginner,
    Intermediate,
    Advanced,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Beginner, Level::Intermediate, Level::Advanced];

    pub fn label(self) -> &'static str {
        match self {
            Level::Beginner => "Beginner",
            Level::Intermediate => "Intermediate",
            Level::Advanced => "Advanced",
        }
    }
}

/// Identifies one subtopic within a path, e.g. "2.0" for topic 3, subtopic 1.
pub type SubtopicKey = String;

pub fn subtopic_key(topic: usize, subtopic: usize) -> SubtopicKey {
    format!("{topic}.{subtopic}")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningPath {
    #[serde(default = "new_path_id")]
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub estimated_time: String,
    #[serde(default)]
    pub topics: Vec<Topic>,
    /// The documentation page the path was generated from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// The Gemini model that generated the path, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    /// Subtopics the learner has finished.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub completed: BTreeSet<SubtopicKey>,
    /// When each finished subtopic was finished (Unix seconds).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub completed_at: BTreeMap<SubtopicKey, i64>,
    /// When the path was last opened (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened: Option<i64>,
    /// While the path is still being written: what is happening now, e.g.
    /// "Writing topic 3 of 6". Never saved.
    #[serde(skip)]
    pub building: Option<String>,
    /// Card positions the learner dragged, by node key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub positions: BTreeMap<String, [f32; 2]>,
}

pub fn new_path_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl LearningPath {
    pub fn subtopic_count(&self) -> usize {
        self.topics.iter().map(|t| t.subtopics.len()).sum()
    }

    /// "4 topics · 12 subtopics"
    pub fn summary(&self) -> String {
        format!(
            "{} {} · {} {}",
            self.topics.len(),
            plural(self.topics.len(), "topic"),
            self.subtopic_count(),
            plural(self.subtopic_count(), "subtopic"),
        )
    }

    /// A stable index into the theme's 8 chart hues, so each path keeps its
    /// own color across the sidebar, cards and map.
    pub fn hue(&self) -> usize {
        let hash = self
            .id
            .bytes()
            .fold(2_166_136_261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16_777_619));
        hash as usize % 8
    }

    /// When every subtopic has numeric hours, rewrites all estimates from
    /// them so topic and path totals always add up.
    pub fn normalize_times(&mut self) {
        let all_known = self
            .topics
            .iter()
            .flat_map(|t| &t.subtopics)
            .all(|s| s.estimated_hours.is_some());
        if !all_known || self.subtopic_count() == 0 {
            // Older paths only have display strings; still keep the total in
            // step with the topics when every topic's estimate can be read.
            let topic_hours: Option<f32> = self.topics.iter().map(|t| parse_hours(&t.estimated_time)).sum();
            if let Some(total) = topic_hours.filter(|_| !self.topics.is_empty()) {
                self.estimated_time = format_hours(total);
            }
            return;
        }
        let mut path_hours = 0.;
        for topic in &mut self.topics {
            let mut topic_hours = 0.;
            for sub in &mut topic.subtopics {
                let hours = sub.estimated_hours.unwrap_or_default().max(0.);
                sub.estimated_time = format_hours(hours);
                topic_hours += hours;
            }
            topic.estimated_time = format_hours(topic_hours);
            path_hours += topic_hours;
        }
        self.estimated_time = format_hours(path_hours);
    }

    pub fn is_done(&self, topic: usize, subtopic: usize) -> bool {
        self.completed.contains(&subtopic_key(topic, subtopic))
    }

    /// (finished, total) subtopics.
    pub fn progress(&self) -> (usize, usize) {
        let done = (0..self.topics.len())
            .flat_map(|ti| (0..self.topics[ti].subtopics.len()).map(move |si| (ti, si)))
            .filter(|(ti, si)| self.is_done(*ti, *si))
            .count();
        (done, self.subtopic_count())
    }

    pub fn topic_progress(&self, topic: usize) -> (usize, usize) {
        let total = self.topics[topic].subtopics.len();
        let done = (0..total).filter(|si| self.is_done(topic, *si)).count();
        (done, total)
    }

    /// Every subtopic in path order.
    pub fn subtopics_in_order(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.topics
            .iter()
            .enumerate()
            .flat_map(|(ti, topic)| (0..topic.subtopics.len()).map(move |si| (ti, si)))
    }

    /// The subtopic after (`offset` 1) or before (`offset` -1) this one.
    pub fn step_from(&self, at: (usize, usize), offset: isize) -> Option<(usize, usize)> {
        let order: Vec<_> = self.subtopics_in_order().collect();
        let ix = order.iter().position(|s| *s == at)?;
        order.get(ix.checked_add_signed(offset)?).copied()
    }

    /// The first unfinished subtopic of one topic.
    pub fn next_in_topic(&self, topic: usize) -> Option<(usize, usize)> {
        let count = self.topics.get(topic)?.subtopics.len();
        (0..count).find(|si| !self.is_done(topic, *si)).map(|si| (topic, si))
    }

    /// The first unfinished subtopic, in path order.
    pub fn next_subtopic(&self) -> Option<(usize, usize)> {
        self.topics.iter().enumerate().find_map(|(ti, topic)| {
            (0..topic.subtopics.len())
                .find(|si| !self.is_done(ti, *si))
                .map(|si| (ti, si))
        })
    }
}

/// Reads a display estimate back: "~2.5 hours" -> 2.5, "~45 min" -> 0.75.
pub fn parse_hours(text: &str) -> Option<f32> {
    let mut words = text.trim().trim_start_matches('~').split_whitespace();
    let number: f32 = words.next()?.parse().ok()?;
    match words.next().unwrap_or("hours") {
        unit if unit.starts_with("min") => Some(number / 60.),
        unit if unit.starts_with('h') => Some(number),
        _ => None,
    }
}

/// "~45 min", "~1 hour", "~2.5 hours", "~28 hours"
pub fn format_hours(hours: f32) -> String {
    if hours < 0.95 {
        let minutes = ((hours * 60. / 5.).round() * 5.).max(5.);
        return format!("~{minutes:.0} min");
    }
    let rounded = (hours * 2.).round() / 2.;
    if rounded == 1. {
        "~1 hour".into()
    } else if rounded.fract() == 0. {
        format!("~{rounded:.0} hours")
    } else {
        format!("~{rounded:.1} hours")
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

pub fn sample_paths() -> Vec<LearningPath> {
    serde_json::from_str(crate::assets::SAMPLES_JSON).expect("bundled samples.json is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_parse_with_camel_case_fields() {
        let paths = sample_paths();
        assert_eq!(paths.len(), 3);
        let rust = &paths[0];
        assert_eq!(rust.estimated_time, "~28 hours");
        let first = &rust.topics[0].subtopics[0];
        assert_eq!(first.technologies_and_concepts, ["Future", "Poll", "Waker"]);
        assert!(!first.resources.is_empty());
    }

    #[test]
    fn missing_id_and_optional_lists_get_defaults() {
        let json = r#"{"name":"X","description":"d","estimatedTime":"1h","topics":[
            {"name":"T","description":"d","estimatedTime":"1h","subtopics":[
                {"name":"S","description":"d","estimatedTime":"1h"}]}]}"#;
        let path: LearningPath = serde_json::from_str(json).unwrap();
        assert!(!path.id.is_empty());
        assert!(path.topics[0].subtopics[0].prerequisites.is_empty());
        assert!(path.completed.is_empty() && path.level.is_none());
        assert_eq!(path.summary(), "1 topic · 1 subtopic");
    }

    #[test]
    fn stepping_crosses_topic_boundaries() {
        let path = sample_paths().remove(2); // 3 topics, 2 subtopics each
        assert_eq!(path.step_from((0, 1), 1), Some((1, 0)));
        assert_eq!(path.step_from((1, 0), -1), Some((0, 1)));
        assert_eq!(path.step_from((0, 0), -1), None);
        assert_eq!(path.step_from((2, 1), 1), None);
        assert_eq!(path.next_in_topic(1), Some((1, 0)));
    }

    #[test]
    fn display_estimates_parse_back() {
        assert_eq!(parse_hours("~2.5 hours"), Some(2.5));
        assert_eq!(parse_hours("~1 hour"), Some(1.0));
        assert_eq!(parse_hours("~45 min"), Some(0.75));
        assert_eq!(parse_hours("soon"), None);
        let mut path = sample_paths().remove(0);
        path.topics[0].estimated_time = "~17 hours".into();
        path.normalize_times();
        assert_eq!(path.estimated_time, "~40 hours", "17 + 7 + 8 + 8");
    }

    #[test]
    fn hours_format_readably() {
        assert_eq!(format_hours(0.5), "~30 min");
        assert_eq!(format_hours(1.0), "~1 hour");
        assert_eq!(format_hours(2.4), "~2.5 hours");
        assert_eq!(format_hours(28.0), "~28 hours");
    }

    #[test]
    fn totals_are_computed_from_subtopic_hours() {
        let mut path = sample_paths().remove(2);
        for topic in &mut path.topics {
            for sub in &mut topic.subtopics {
                sub.estimated_hours = Some(1.5);
            }
        }
        path.normalize_times();
        assert_eq!(path.topics[0].estimated_time, "~3 hours");
        assert_eq!(path.estimated_time, "~9 hours");
    }

    #[test]
    fn progress_and_next_step_follow_completion() {
        let mut path = sample_paths().remove(2);
        assert_eq!(path.next_subtopic(), Some((0, 0)));
        path.completed.insert(subtopic_key(0, 0));
        path.completed.insert(subtopic_key(0, 1));
        assert_eq!(path.progress(), (2, 6));
        assert_eq!(path.topic_progress(0), (2, 2));
        assert_eq!(path.next_subtopic(), Some((1, 0)));
    }
}
