//! Study statistics for the Today view: this week's finished subtopics and
//! hours, the day streak, and recent completions. Days are local days.

use std::collections::BTreeSet;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::model::{LearningPath, parse_hours};

pub struct Completion {
    pub path_id: String,
    pub path_name: String,
    pub topic: usize,
    pub subtopic: usize,
    pub name: String,
    pub at: i64,
}

pub struct Summary {
    pub done_this_week: usize,
    pub hours_this_week: f32,
    /// Finished in the seven days before this week, for the trend chip.
    pub done_last_week: usize,
    /// Consecutive days with at least one finished subtopic, counting back
    /// from today (or from yesterday, so a streak survives until tonight).
    pub streak_days: u32,
    /// Newest first.
    pub recent: Vec<Completion>,
}

pub fn now_secs() -> i64 {
    Timestamp::now().as_second()
}

fn local_date(secs: i64, tz: &TimeZone) -> Option<Date> {
    Some(Timestamp::from_second(secs).ok()?.to_zoned(tz.clone()).date())
}

pub fn summarize(paths: &[LearningPath], now: i64, tz: &TimeZone) -> Summary {
    let mut completions = Vec::new();
    for path in paths {
        for (ti, topic) in path.topics.iter().enumerate() {
            for (si, sub) in topic.subtopics.iter().enumerate() {
                let key = crate::model::subtopic_key(ti, si);
                if let (true, Some(at)) = (path.completed.contains(&key), path.completed_at.get(&key)) {
                    completions.push((
                        Completion {
                            path_id: path.id.clone(),
                            path_name: path.name.clone(),
                            topic: ti,
                            subtopic: si,
                            name: sub.name.clone(),
                            at: *at,
                        },
                        sub.estimated_hours.or_else(|| parse_hours(&sub.estimated_time)).unwrap_or(0.),
                    ));
                }
            }
        }
    }
    let today = local_date(now, tz);
    let week_start = today.and_then(|d| d.checked_sub(jiff::Span::new().days(6)).ok());
    let in_week = |at: i64| match (local_date(at, tz), week_start, today) {
        (Some(day), Some(start), Some(end)) => day >= start && day <= end,
        _ => false,
    };
    let (done_this_week, hours_this_week) = completions
        .iter()
        .filter(|(c, _)| in_week(c.at))
        .fold((0, 0.), |(n, h), (_, hours)| (n + 1, h + hours));
    let last_week = week_start.and_then(|start| Some((start.checked_sub(jiff::Span::new().days(7)).ok()?, start)));
    let done_last_week = completions
        .iter()
        .filter(|(c, _)| match (local_date(c.at, tz), last_week) {
            (Some(day), Some((from, until))) => day >= from && day < until,
            _ => false,
        })
        .count();

    let days: BTreeSet<Date> = completions.iter().filter_map(|(c, _)| local_date(c.at, tz)).collect();
    let mut streak_days = 0;
    if let Some(today) = today {
        let yesterday = today.yesterday().ok();
        let mut day = if days.contains(&today) { Some(today) } else { yesterday.filter(|d| days.contains(d)) };
        while let Some(d) = day.filter(|d| days.contains(d)) {
            streak_days += 1;
            day = d.yesterday().ok();
        }
    }

    let mut recent: Vec<Completion> = completions.into_iter().map(|(c, _)| c).collect();
    recent.sort_by_key(|c| std::cmp::Reverse(c.at));
    recent.truncate(5);
    Summary {
        done_this_week,
        hours_this_week,
        done_last_week,
        streak_days,
        recent,
    }
}

/// "just now", "12 min ago", "3 h ago", "yesterday", "4 days ago".
pub fn ago(at: i64, now: i64) -> String {
    let secs = (now - at).max(0);
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        86_400..172_800 => "yesterday".into(),
        _ => format!("{} days ago", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_paths, subtopic_key};

    const DAY: i64 = 86_400;

    fn finish(path: &mut LearningPath, ti: usize, si: usize, at: i64) {
        let key = subtopic_key(ti, si);
        path.completed.insert(key.clone());
        path.completed_at.insert(key, at);
    }

    #[test]
    fn week_hours_streak_and_recent() {
        let tz = TimeZone::UTC;
        let now = 1_790_000_000 - 1_790_000_000 % DAY + 12 * 3600; // noon UTC
        let mut path = sample_paths().remove(0); // subtopics are "~2 hours" etc.
        finish(&mut path, 0, 0, now - 3600); // today, ~2 h
        finish(&mut path, 0, 1, now - DAY); // yesterday, ~2 h
        finish(&mut path, 0, 2, now - 2 * DAY); // two days ago, ~1 h
        finish(&mut path, 1, 0, now - 20 * DAY); // long ago, outside the week
        let summary = summarize(&[path], now, &tz);
        assert_eq!(summary.done_this_week, 3);
        assert!((summary.hours_this_week - 5.).abs() < 1e-3);
        assert_eq!(summary.streak_days, 3);
        assert_eq!(summary.recent[0].name, "Futures & the poll model");
        assert_eq!(summary.recent.len(), 4);
    }

    #[test]
    fn a_streak_survives_until_the_day_ends() {
        let tz = TimeZone::UTC;
        let now = 1_790_000_000 - 1_790_000_000 % DAY + 9 * 3600;
        let mut path = sample_paths().remove(0);
        finish(&mut path, 0, 0, now - DAY);
        assert_eq!(summarize(&[path.clone()], now, &tz).streak_days, 1);
        finish(&mut path, 0, 1, now - 3 * DAY);
        assert_eq!(summarize(&[path], now, &tz).streak_days, 1, "a gap ends the streak");
    }

    #[test]
    fn last_week_is_the_seven_days_before_this_week() {
        let tz = TimeZone::UTC;
        let now = 1_790_000_000 - 1_790_000_000 % DAY + 12 * 3600;
        let mut path = sample_paths().remove(0);
        finish(&mut path, 0, 0, now - 3600); // this week
        finish(&mut path, 0, 1, now - 7 * DAY); // last week, first day
        finish(&mut path, 0, 2, now - 13 * DAY); // last week, last day
        finish(&mut path, 1, 0, now - 14 * DAY); // before last week
        let summary = summarize(&[path], now, &tz);
        assert_eq!(summary.done_this_week, 1);
        assert_eq!(summary.done_last_week, 2);
    }

    #[test]
    fn relative_times_read_naturally() {
        assert_eq!(ago(100, 130), "just now");
        assert_eq!(ago(0, 720), "12 min ago");
        assert_eq!(ago(0, 3 * 3600), "3 h ago");
        assert_eq!(ago(0, DAY + 10), "yesterday");
        assert_eq!(ago(0, 4 * DAY), "4 days ago");
    }
}
