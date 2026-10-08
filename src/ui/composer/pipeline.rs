//! Making a path in two steps. Drafting finds and reads the official docs,
//! then asks Gemini for an outline the learner reviews. Building streams the
//! full path; topics reach the map as soon as each one is written.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use futures::StreamExt;
use gpui::{AppContext, AsyncApp, Context, WeakEntity};

use super::{Building, ComposerEvent, ComposerView, Failure, Phase, Request, Retry, Review};
use crate::config::Config;
use crate::model::{LearningPath, Topic, new_path_id};
use crate::services::gemini::{GeminiError, Intent, Learner};
use crate::services::stream::{self, Outline};
use crate::services::{self, docs, links, partial};
use crate::ui::generation::{Generation, Stage};

impl ComposerView {
    pub(super) fn start_draft(&mut self, topic: String, intent: Intent, models: Vec<String>, cx: &mut Context<Self>) {
        let config = cx.global::<Config>().clone();
        if config.gemini_api_key.is_none() {
            self.error =
                Some("GEMINI_API_KEY isn't set. Add it to .env next to Cargo.toml and restart.".into());
            cx.notify();
            return;
        }
        let learner = Learner {
            level: self.level,
            hours_per_week: self.pace,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let task = cx.spawn({
            let (topic, intent, cancelled) = (topic.clone(), intent.clone(), cancelled.clone());
            async move |this, cx| {
                let request = Request {
                    topic,
                    intent,
                    learner,
                    models,
                    context: String::new(),
                    source: None,
                    nav: Vec::new(),
                };
                let result = Self::run_draft(&this, request, config, cancelled, cx).await;
                this.update(cx, |this, cx| this.finish_draft(result, cx)).ok();
            }
        });
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        self.error = None;
        self.failure = None;
        self.phase = Phase::Drafting(Generation {
            topic,
            stage: Stage::SearchingDocs,
            started: Instant::now(),
            source: None,
            model: None,
            pages_read: 0,
            contents_entries: 0,
            cancelled,
            notes: Vec::new(),
            _task: task,
            _ticker: ticker,
        });
        self.pending_intent = intent;
        cx.notify();
    }

    /// Docs, their outline, then Gemini's draft. Doc problems are noted and
    /// skipped; the outline is still drafted from Gemini's own knowledge.
    async fn run_draft(
        this: &WeakEntity<Self>,
        mut request: Request,
        config: Config,
        cancelled: Arc<AtomicBool>,
        cx: &mut AsyncApp,
    ) -> Result<Review> {
        let agent = services::http_agent();
        let update = |cx: &mut AsyncApp, f: &dyn Fn(&mut Generation)| {
            this.update(cx, |this, cx| {
                if let Phase::Drafting(generation) = &mut this.phase {
                    f(generation);
                    cx.notify();
                }
            })
        };
        let note = |cx: &mut AsyncApp, note: String| update(cx, &|g: &mut Generation| g.notes.push(note.clone()));
        let topic = request.topic.clone();

        let url = match (docs::known_docs_url(&topic), config.serpapi_api_key.clone()) {
            (Some(url), _) => Some(url.to_string()),
            (None, None) => {
                note(cx, "SERPAPI_API_KEY isn't set, so no docs were searched.".into())?;
                None
            }
            (None, Some(key)) => {
                let (agent, query) = (agent.clone(), topic.clone());
                match cx.background_spawn(async move { docs::search_docs(&agent, &query, &key) }).await {
                    Ok(results) => {
                        let picked = docs::pick_official(&topic, &results);
                        if picked.is_none() {
                            note(cx, "No official documentation found; continuing without docs.".into())?;
                        }
                        picked
                    }
                    Err(error) => {
                        note(cx, format!("Doc search failed ({error:#}); continuing without docs."))?;
                        None
                    }
                }
            }
        };

        update(cx, &|g: &mut Generation| {
            g.stage = Stage::ReadingDocs;
            g.source = url.clone();
        })?;
        if let Some(url) = url.clone() {
            let (agent, topic) = (agent.clone(), topic.clone());
            match cx.background_spawn(async move { docs::gather_context(&agent, &url, &topic) }).await {
                Ok(context) => {
                    let (pages, entries) = (context.pages.len(), context.nav.len());
                    update(cx, &|g: &mut Generation| {
                        g.pages_read = pages;
                        g.contents_entries = entries;
                    })?;
                    request.context = context.text;
                    request.nav = context.nav;
                }
                Err(error) => {
                    note(cx, format!("Couldn't read the docs ({error:#}); continuing without them."))?;
                }
            }
        }
        request.source = url;

        update(cx, &|g: &mut Generation| g.stage = Stage::DraftingOutline)?;
        let api_key = config.gemini_api_key.clone().ok_or_else(|| anyhow!("GEMINI_API_KEY isn't set"))?;
        let prompt = stream::outline_prompt(&topic, request.learner, &request.intent, &request.context, &request.nav);
        let models = request.models.clone();
        for (ix, model) in models.iter().enumerate() {
            update(cx, &|g: &mut Generation| g.model = Some(model.clone()))?;
            let attempt = cx
                .background_spawn({
                    let (agent, api_key, model, prompt, cancelled) =
                        (agent.clone(), api_key.clone(), model.clone(), prompt.clone(), cancelled.clone());
                    let nav = request.nav.clone();
                    async move { stream::draft_outline(&agent, &api_key, &model, &prompt, &nav, &cancelled) }
                })
                .await;
            match attempt {
                Ok(outline) => {
                    // The build should start with the model that answered.
                    request.models.rotate_left(ix);
                    return Ok(Review { request, outline });
                }
                Err(GeminiError::Unavailable(error)) if ix + 1 < models.len() && !cancelled.load(Ordering::Relaxed) => {
                    note(cx, format!("{error:#}. Switched to {}.", models[ix + 1]))?;
                }
                Err(error) => return Err(error.into_inner()),
            }
        }
        Err(anyhow!("no Gemini model is configured"))
    }

    fn finish_draft(&mut self, result: Result<Review>, cx: &mut Context<Self>) {
        let Phase::Drafting(generation) = std::mem::replace(&mut self.phase, Phase::Idle) else {
            return;
        };
        match result {
            Ok(review) => self.phase = Phase::Review(Box::new(review)),
            Err(error) => {
                let intent = std::mem::take(&mut self.pending_intent);
                self.fail(Retry::Draft { topic: generation.topic, intent }, error, generation.model, cx);
            }
        }
        cx.notify();
    }

    /// Opens the map on the outline's topics and streams in the rest.
    pub(super) fn start_build(&mut self, review: Review, cx: &mut Context<Self>) {
        let Some(api_key) = cx.global::<Config>().gemini_api_key.clone() else {
            return;
        };
        let draft = skeleton(&review, new_path_id());
        let (path_id, topic) = (draft.id.clone(), review.request.topic.clone());
        cx.emit(ComposerEvent::Started(Box::new(draft.clone())));
        let cancelled = Arc::new(AtomicBool::new(false));
        let task = cx.spawn({
            let (review, cancelled) = (review.clone(), cancelled.clone());
            async move |this, cx| {
                let result = Self::run_build(&this, &review, draft, api_key, cancelled, cx).await;
                this.update(cx, |this, cx| this.finish_build(review, result, cx)).ok();
            }
        });
        for field in [&self.input, &self.goal, &self.background] {
            field.update(cx, |input, cx| input.set_text("", cx));
        }
        self.phase = Phase::Building(Building {
            path_id,
            topic,
            cancelled,
            _task: task,
        });
        cx.notify();
    }

    async fn run_build(
        this: &WeakEntity<Self>,
        review: &Review,
        mut draft: LearningPath,
        api_key: String,
        cancelled: Arc<AtomicBool>,
        cx: &mut AsyncApp,
    ) -> Result<LearningPath> {
        let Review { request, outline } = review;
        let agent = services::http_agent();
        let prompt = stream::build_prompt(&request.topic, request.learner, &request.intent, &request.context, outline);
        let total = outline.topics.len();
        let emit = |cx: &mut AsyncApp, path: &LearningPath| {
            let path = Box::new(path.clone());
            this.update(cx, |_, cx| cx.emit(ComposerEvent::Progress(path))).ok();
        };

        let mut finished = None;
        for (ix, model) in request.models.iter().enumerate() {
            draft.generated_by = Some(model.clone());
            let (sender, mut fragments) = futures::channel::mpsc::unbounded::<String>();
            let streaming = cx.background_spawn({
                let (agent, api_key, model, prompt, cancelled) =
                    (agent.clone(), api_key.clone(), model.clone(), prompt.clone(), cancelled.clone());
                async move {
                    stream::stream(&agent, &api_key, &model, &prompt, &cancelled, &mut |text| {
                        sender.unbounded_send(text.to_string()).ok();
                    })
                }
            });
            let (mut text, mut shown) = (String::new(), 0);
            while let Some(fragment) = fragments.next().await {
                text.push_str(&fragment);
                let written = partial::parse(&text).topics;
                if written.len() > shown {
                    shown = written.len();
                    fill(&mut draft, outline, &written);
                    draft.building = Some(format!("Writing topic {} of {total}", (shown + 1).min(total)));
                    emit(cx, &draft);
                }
            }
            match streaming.await {
                Ok(text) => {
                    finished = Some(stream::finish(&text, request.learner, outline)?);
                    break;
                }
                // Switching models is only safe before anything was shown.
                Err(GeminiError::Unavailable(_)) if shown == 0 && ix + 1 < request.models.len() => continue,
                Err(error) => return Err(error.into_inner()),
            }
        }
        let mut path = finished.ok_or_else(|| anyhow!("no Gemini model is configured"))?;
        path.id = draft.id.clone();
        path.source_url = request.source.clone();
        path.topic_query = Some(request.topic.clone());
        path.generated_by = draft.generated_by.clone();
        path.building = Some("Checking resource links".into());
        emit(cx, &path);
        let mut path = cx
            .background_spawn(async move {
                links::drop_broken(&mut path);
                path
            })
            .await;
        path.building = None;
        Ok(path)
    }

    fn finish_build(&mut self, review: Review, result: Result<LearningPath>, cx: &mut Context<Self>) {
        let Phase::Building(building) = std::mem::replace(&mut self.phase, Phase::Idle) else {
            return;
        };
        match result {
            Ok(path) => cx.emit(ComposerEvent::Finished(Box::new(path))),
            Err(error) => {
                cx.emit(ComposerEvent::Failed(building.path_id));
                let model = review.request.models.first().cloned();
                self.fail(Retry::Build(Box::new(review)), error, model, cx);
            }
        }
        cx.notify();
    }

    fn fail(&mut self, retry: Retry, error: anyhow::Error, model: Option<String>, cx: &mut Context<Self>) {
        let config = cx.global::<Config>();
        let used = model.unwrap_or_else(|| config.gemini_model.clone());
        self.failure = Some(Failure {
            message: format!("{error:#}").into(),
            alternate: config.gemini_models().into_iter().find(|m| *m != used),
            retry,
        });
    }

    /// Stops drafting or building at the next step; nothing more is retried.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Drafting(generation) => {
                generation.cancelled.store(true, Ordering::Relaxed);
                cx.emit(ComposerEvent::Cancelled(generation.topic));
            }
            Phase::Building(building) => {
                building.cancelled.store(true, Ordering::Relaxed);
                cx.emit(ComposerEvent::Failed(building.path_id));
                cx.emit(ComposerEvent::Cancelled(building.topic));
            }
            other => self.phase = other,
        }
        cx.notify();
    }

    pub(super) fn retry(&mut self, with_alternate: bool, cx: &mut Context<Self>) {
        let Some(failure) = self.failure.take() else {
            return;
        };
        let reorder = |models: &mut Vec<String>| {
            if with_alternate && let Some(alternate) = &failure.alternate {
                models.retain(|m| m != alternate);
                models.insert(0, alternate.clone());
            }
        };
        match failure.retry {
            Retry::Draft { topic, intent } => {
                let mut models = cx.global::<Config>().gemini_models();
                reorder(&mut models);
                self.start_draft(topic, intent, models, cx);
            }
            Retry::Build(mut review) => {
                reorder(&mut review.request.models);
                self.start_build(*review, cx);
            }
        }
    }
}

/// The path as it first appears: the reviewed topics, no subtopics yet.
fn skeleton(review: &Review, id: String) -> LearningPath {
    let Review { request, outline } = review;
    LearningPath {
        id,
        name: outline.name.clone(),
        description: outline.description.clone(),
        estimated_time: String::new(),
        topics: outline
            .topics
            .iter()
            .map(|t| Topic {
                name: t.name.clone(),
                description: t.description.clone(),
                estimated_time: String::new(),
                subtopics: Vec::new(),
            })
            .collect(),
        source_url: request.source.clone(),
        topic_query: Some(request.topic.clone()),
        generated_by: request.models.first().cloned(),
        level: Some(request.learner.level),
        completed: Default::default(),
        completed_at: Default::default(),
        last_opened: None,
        building: Some(format!("Writing topic 1 of {}", outline.topics.len())),
        positions: Default::default(),
    }
}

/// Puts the topics written so far into the draft, under their approved names.
fn fill(draft: &mut LearningPath, outline: &Outline, written: &[Topic]) {
    for (ix, topic) in written.iter().enumerate().take(draft.topics.len()) {
        let mut topic = topic.clone().with_display_times();
        topic.name = outline.topics[ix].name.clone();
        draft.topics[ix] = topic;
    }
}
