//! Reading a learning path out of JSON that is still being written. Gemini
//! streams the path as one JSON document; each time a topic object closes,
//! it can be shown, long before the whole answer arrives.

use serde::Deserialize;

use crate::model::Topic;

/// The fields that precede `topics` (the schema orders them first).
#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct Header {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// What a JSON prefix holds so far.
#[derive(Debug, Default, PartialEq)]
pub struct Partial {
    pub header: Option<Header>,
    /// Every topic whose object has closed, in order.
    pub topics: Vec<Topic>,
}

/// Parses a prefix of `{"name":…,"description":…,"topics":[{…},{…`.
pub fn parse(prefix: &str) -> Partial {
    let mut partial = Partial::default();
    let Some(key) = find_key(prefix, "topics") else {
        return partial;
    };
    let Some(open) = prefix[key..].find('[').map(|i| key + i) else {
        return partial;
    };
    // Everything before the array, closed off, is the header.
    let head = format!("{}\"topics\":[]}}", &prefix[..key]);
    partial.header = serde_json::from_str(head.trim_start_matches("```json").trim()).ok();

    let body = &prefix[open + 1..];
    let (mut depth, mut start, mut in_string, mut escaped) = (0usize, None, false, false);
    for (i, ch) in body.char_indices() {
        if in_string {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(from) = start.take()
                    && let Ok(topic) = serde_json::from_str::<Topic>(&body[from..=i])
                {
                    partial.topics.push(topic);
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    partial
}

/// Byte offset of `"key"` used as an object key at any depth, skipping
/// occurrences inside string values.
fn find_key(text: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let (mut in_string, mut escaped) = (false, false);
    for (i, ch) in text.char_indices() {
        if !in_string && text[i..].starts_with(&needle) {
            let after = text[i + needle.len()..].trim_start();
            if after.starts_with(':') {
                return Some(i);
            }
        }
        if in_string {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
        } else if ch == '"' {
            in_string = true;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{"name":"Rust","description":"A \"quoted\" path {not a brace}","topics":[
        {"name":"One","description":"d","subtopics":[{"name":"a","description":"has } brace","estimatedHours":1,
          "technologiesAndConcepts":[],"prerequisites":[],"resources":[]}]},
        {"name":"Two","description":"d","subtopics":[]}]}"#;

    #[test]
    fn closed_topics_appear_as_the_stream_grows() {
        let cut = FULL.find("{\"name\":\"Two\"").unwrap();
        let early = parse(&FULL[..cut + 10]);
        assert_eq!(early.header.as_ref().unwrap().name, "Rust");
        assert_eq!(early.topics.len(), 1);
        assert_eq!(early.topics[0].subtopics[0].description, "has } brace");

        let done = parse(FULL);
        assert_eq!(done.topics.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["One", "Two"]);
        assert_eq!(done.header.unwrap().description, "A \"quoted\" path {not a brace}");
    }

    #[test]
    fn nothing_before_the_topics_key() {
        assert_eq!(parse(r#"{"name":"Ru"#), Partial::default());
        assert_eq!(parse("").topics.len(), 0);
    }

    #[test]
    fn a_topics_word_inside_a_string_is_not_the_key() {
        let text = r#"{"name":"\"topics\": a trick","description":"x","topics":[{"name":"T","description":"d"}"#;
        let partial = parse(text);
        assert_eq!(partial.header.unwrap().name, "\"topics\": a trick");
        assert_eq!(partial.topics.len(), 1);
    }
}
