//! "Expand in depth": regenerates one topic with deeper subtopics, using the
//! same model fallback and link checks as a full generation.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Result, anyhow};
use gpui::{AppContext, Context, Task};

use super::{MapEvent, MindMapView};
use crate::config::Config;
use crate::model::Topic;
use crate::services::gemini::{self, GeminiError, Learner};
use crate::services::{self, links};

/// A topic being expanded; dropping it abandons the request.
pub(super) struct Expanding {
    pub topic: usize,
    _task: Task<()>,
}

impl MindMapView {
    pub(super) fn expand_topic(&mut self, topic_ix: usize, cx: &mut Context<Self>) {
        if self.expanding.is_some() {
            return;
        }
        let config = cx.global::<Config>().clone();
        let Some(api_key) = config.gemini_api_key.clone() else {
            cx.emit(MapEvent::Failed {
                title: "Couldn't expand the topic".into(),
                body: "GEMINI_API_KEY isn't set.".into(),
            });
            return;
        };
        let path = self.path.clone();
        let learner = Learner {
            level: path.level.unwrap_or_default(),
            hours_per_week: None,
        };
        let models = config.gemini_models();
        let task = cx.spawn(async move |this, cx| {
            let agent = services::http_agent();
            let cancelled = Arc::new(AtomicBool::new(false));
            let mut result: Result<Topic> = Err(anyhow!("no Gemini model is configured"));
            for model in models {
                let attempt = cx
                    .background_spawn({
                        let (agent, api_key, path, cancelled) =
                            (agent.clone(), api_key.clone(), path.clone(), cancelled.clone());
                        async move {
                            gemini::expand_topic(&agent, &api_key, &model, &path, topic_ix, learner, &cancelled)
                        }
                    })
                    .await;
                match attempt {
                    Ok(topic) => {
                        result = Ok(topic);
                        break;
                    }
                    Err(GeminiError::Unavailable(error)) => result = Err(error),
                    Err(GeminiError::Failed(error)) => {
                        result = Err(error);
                        break;
                    }
                }
            }
            let result = match result {
                Ok(topic) => Ok(cx
                    .background_spawn(async move { finish_topic(topic) })
                    .await),
                Err(error) => Err(error),
            };
            this.update(cx, |this, cx| this.finish_expand(topic_ix, result, cx))
                .ok();
        });
        self.expanding = Some(Expanding {
            topic: topic_ix,
            _task: task,
        });
        cx.notify();
    }

    fn finish_expand(&mut self, topic_ix: usize, result: Result<Topic>, cx: &mut Context<Self>) {
        self.expanding = None;
        match result {
            Ok(topic) => {
                let (name, count) = (topic.name.clone(), topic.subtopics.len());
                let id = self.path_id.clone();
                self.store
                    .update(cx, |store, cx| store.replace_topic(&id, topic_ix, topic, cx));
                cx.emit(MapEvent::Done {
                    title: "Topic expanded".into(),
                    body: format!("\u{201c}{name}\u{201d} now has {count} subtopics."),
                });
            }
            Err(error) => cx.emit(MapEvent::Failed {
                title: "Couldn't expand the topic".into(),
                body: format!("{error:#}"),
            }),
        }
        cx.notify();
    }
}

/// Display times from the numeric hours, and no dead links.
fn finish_topic(topic: Topic) -> Topic {
    let mut topic = topic.with_display_times();
    links::drop_broken_in(topic.subtopics.iter_mut());
    topic
}
