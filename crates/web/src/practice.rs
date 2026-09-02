//! The practice screen: the text, the keystrokes, the figures, the keyboard.
//!
//! The whole typing loop runs here in the browser. Nothing waits on the network:
//! the exercise is generated locally from a random seed, and the socket — if it
//! is up at all — only carries the seed and the keystroke stream so the server
//! can score the run for the leaderboard. Pull the plug mid-exercise and the
//! typist notices nothing.

use leptos::{ev, prelude::*};
use typing_core::{
    corpus::Corpus,
    exercise::{self, Exercise, LINE_END_MARK},
    goals::Module,
    lesson::{klavaro_lessons, Lesson},
    load_layout,
    stats::Score,
    typist::{CharState, Correction, Key, Press, Typist},
    DEFAULT_LAYOUT,
};

use crate::{
    board::BoardTable,
    i18n::Msg,
    keyboard::VirtualKeyboard,
    protocol::{BoardEntry, ClientMessage, ServerMessage, Touch, TouchKind, TOUCH_BATCH_SIZE},
    settings::{ProgressStore, Settings, LAST_LESSON},
    socket::{self, Status},
};

/// Where the visitor is in a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Text is on screen, waiting for the first keystroke.
    Ready,
    /// Typing, clock running.
    Typing,
    /// Finished and scored.
    Done,
}

/// The practice screen.
#[component]
pub fn Practice() -> impl IntoView {
    let settings = expect_context::<Settings>();
    let progress = expect_context::<ProgressStore>();
    let lessons = StoredValue::new(klavaro_lessons());

    // --- exercise and run state ------------------------------------------
    let exercise = RwSignal::new(None::<Exercise>);
    let typist = RwSignal::new(None::<Typist>);
    let phase = RwSignal::new(Phase::Ready);
    let score = RwSignal::new(None::<Score>);
    let problem = RwSignal::new(None::<String>);

    // --- corpus, fetched per language -------------------------------------
    let corpus = RwSignal::new(None::<Corpus>);

    // --- server relationship ----------------------------------------------
    let status = RwSignal::new(Status::Idle);
    let board = RwSignal::new(Vec::<BoardEntry>::new());
    let publishable = RwSignal::new(false);
    let would_rank = RwSignal::new(None::<u32>);
    let published = RwSignal::new(false);
    let personal_best = RwSignal::new(false);
    let nickname = RwSignal::new(String::new());
    let server_following = RwSignal::new(false);

    // --- timing (client only) ---------------------------------------------
    let start_ms = StoredValue::new(0.0_f64);
    let last_key_ms = StoredValue::new(0.0_f64);
    let outbox = StoredValue::new(Vec::<Touch>::new());
    // Re-read on every keystroke so live figures update without cloning the text.
    let tick = RwSignal::new(0_u32);

    let layout = Memo::new(move |_| {
        load_layout(&settings.layout_name.get())
            .or_else(|| load_layout(DEFAULT_LAYOUT))
            .expect("the default layout is bundled")
    });

    // ---------------------------------------------------------------------
    // Restore preferences, then open the socket. Effects run only in the
    // browser, so neither touches server rendering.
    // ---------------------------------------------------------------------
    Effect::new(move |_| {
        settings.restore();
        progress.restore();
        socket::connect(
            move |message| match message {
                ServerMessage::Following { expected_chars } => {
                    let ours = exercise.read().as_ref().map(|e| e.len_chars() as u32);
                    if ours == Some(expected_chars) {
                        server_following.set(true);
                    } else {
                        // The bundle and the server generated different text, so
                        // reporting would only produce rejections. Practise on.
                        server_following.set(false);
                        problem.set(Some(
                            "This page is out of date — reload to publish results.".into(),
                        ));
                    }
                }
                ServerMessage::Scored {
                    score: scored,
                    publishable: ok,
                    would_rank: rank,
                    ..
                } => {
                    score.set(Some(scored));
                    publishable.set(ok);
                    would_rank.set(rank);
                }
                ServerMessage::Board { entries, .. } => board.set(entries),
                ServerMessage::Problem { detail, .. } => problem.set(Some(detail)),
                ServerMessage::Pong => {}
            },
            move |new_status| status.set(new_status),
        );
    });

    // Fetch practice text whenever the language changes and a module needs it.
    Effect::new(move |_| {
        let language = settings.corpus_language.get();
        let needs_text = matches!(settings.module.get(), Module::Velocity | Module::Fluidness);
        if !needs_text {
            return;
        }
        if corpus
            .read()
            .as_ref()
            .is_some_and(|c| c.language == language)
        {
            return;
        }
        fetch_corpus(language, corpus);
    });

    // --- starting a new exercise -----------------------------------------
    let new_exercise = move || {
        let module = settings.module.get();
        let seed = socket::random_seed();
        let current_layout = layout.get();
        let lesson_number = settings.lesson.get();

        let generated = lessons.with_value(|all| {
            let lesson: Option<&Lesson> = (module == Module::Basic)
                .then(|| all.iter().find(|l| l.number == lesson_number))
                .flatten();
            let held = corpus.read();
            exercise::generate(
                exercise::Request {
                    module,
                    layout: &current_layout,
                    lesson,
                    corpus: held.as_ref(),
                    stop_marks: true,
                },
                seed,
            )
        });

        match generated {
            Ok(generated) => {
                typist.set(Some(Typist::for_module(&generated.text, module)));
                exercise.set(Some(generated));
                phase.set(Phase::Ready);
                score.set(None);
                problem.set(None);
                publishable.set(false);
                would_rank.set(None);
                published.set(false);
                server_following.set(false);
                outbox.set_value(Vec::new());
                tick.set(0);

                socket::send(&ClientMessage::Start {
                    module,
                    layout: settings.layout_name.get(),
                    language: settings.corpus_language.get(),
                    lesson: (module == Module::Basic).then_some(lesson_number),
                    seed,
                });
                settings.persist();
            }
            Err(error) => {
                // Velocity and Fluidness before the text has arrived: say so
                // rather than showing an empty screen.
                typist.set(None);
                exercise.set(None);
                problem.set(Some(error.to_string()));
            }
        }
    };

    // Regenerate when the module, layout, lesson or text language changes.
    Effect::new(move |previous: Option<()>| {
        // Subscribe to everything an exercise depends on.
        let _ = (
            settings.module.get(),
            settings.layout_name.get(),
            settings.lesson.get(),
            corpus.read().as_ref().map(|c| c.language.clone()),
        );
        // Skip nothing: the first run also needs an exercise.
        let _ = previous;
        new_exercise();
    });

    // Follow the board for whatever is being practised.
    Effect::new(move |_| {
        let module = settings.module.get();
        if module.is_ranked() && status.get() == Status::Open {
            socket::send(&ClientMessage::WatchBoard {
                module,
                language: settings.corpus_language.get(),
            });
        }
    });

    // --- keystrokes -------------------------------------------------------
    let handle = window_event_listener(ev::keydown, move |event| {
        if event.ctrl_key() || event.meta_key() || event.alt_key() {
            return; // leave browser shortcuts alone
        }
        let key = match event.key().as_str() {
            "Backspace" => Key::Backspace,
            "Enter" => Key::Char('\n'),
            other => {
                let mut chars = other.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => Key::Char(ch),
                    // "Shift", "ArrowLeft", "F5" and friends.
                    _ => return,
                }
            }
        };
        if phase.get_untracked() == Phase::Done || typist.read_untracked().is_none() {
            return;
        }
        // Space scrolls, Backspace navigates back, Enter submits: all unwanted.
        event.prevent_default();

        let correction = Correction::for_module(settings.module.get_untracked());
        let now = socket::now_ms();
        if phase.get_untracked() == Phase::Ready {
            start_ms.set_value(now);
            last_key_ms.set_value(now);
            phase.set(Phase::Typing);
        }
        let dt_us = ((now - last_key_ms.get_value()).max(0.0) * 1_000.0) as u32;
        last_key_ms.set_value(now);
        let at_us = ((now - start_ms.get_value()).max(0.0) * 1_000.0) as u64;

        let (press, finished) = typist
            .try_update(|held| {
                let held = held.as_mut().expect("checked above");
                let press = held.press(key, at_us);
                (press, held.is_finished())
            })
            .unwrap_or((Press::Ignored, false));

        if press == Press::Ignored {
            return;
        }
        tick.update(|n| *n += 1);

        if let Some(kind) = touch_kind(press, correction) {
            outbox.update_value(|queue| queue.push(Touch { kind, dt_us }));
            let full = outbox.with_value(|queue| queue.len() >= TOUCH_BATCH_SIZE);
            if full {
                flush(outbox);
            }
        }

        if finished {
            flush(outbox);
            phase.set(Phase::Done);
            let local = typist.read_untracked().as_ref().map(|t| t.score());
            // Progress is recorded from the local score and kept in this
            // browser, so it works with or without a connection.
            if let Some(score) = local.as_ref() {
                let improved = progress.record(
                    settings.module.get_untracked(),
                    score,
                    settings.lesson.get_untracked(),
                );
                personal_best.set(improved);
            }
            // Show the local score at once; the server's replaces it when its
            // reply arrives, and that is the one the board uses.
            score.set(local);
            if let Some(session) = typist
                .read_untracked()
                .as_ref()
                .map(|t| t.session().clone())
            {
                if server_following.get_untracked() {
                    socket::send(&ClientMessage::Finish {
                        client_session: session,
                    });
                }
            }
        }
    });
    on_cleanup(move || handle.remove());

    // --- derived views ----------------------------------------------------
    let locale = move || settings.locale.get();
    let next_char = Memo::new(move |_| {
        tick.track();
        typist.read().as_ref().and_then(|t| t.expected())
    });

    view! {
        <section class="practice">
            <ModulePicker />

            {move || {
                problem
                    .get()
                    .map(|detail| view! { <p class="notice notice-problem">{detail}</p> })
            }}
            {move || {
                (status.get() == Status::Closed)
                    .then(|| {
                        view! {
                            <p class="notice">{move || locale().text(Msg::Offline)}</p>
                        }
                    })
            }}

            <TypingSurface typist=typist tick=tick phase=phase />

            <LiveStats typist=typist tick=tick />

            <VirtualKeyboard layout=layout next=next_char />

            {move || {
                (settings.module.get() == Module::Fluidness)
                    .then(|| {
                        view! {
                            <p class="hint">{move || locale().text(Msg::CorrectionRequired)}</p>
                        }
                    })
            }}

            <div class="actions">
                <button class="button" on:click=move |_| new_exercise()>
                    {move || locale().text(Msg::NewExercise)}
                </button>
            </div>

            {move || {
                (phase.get() == Phase::Done)
                    .then(|| {
                        view! {
                            <Results
                                score=score
                                publishable=publishable
                                would_rank=would_rank
                                published=published
                                nickname=nickname
                                personal_best=personal_best
                            />
                        }
                    })
            }}

            {move || {
                settings
                    .module
                    .get()
                    .is_ranked()
                    .then(|| view! { <BoardTable entries=board /> })
            }}
        </section>
    }
}

/// Which reported keystroke, if any, a press corresponds to.
///
/// Delegates to [`Press::counted`] so the client cannot disagree with the server
/// about what a keystroke meant — see the note there about wrong keys.
fn touch_kind(press: Press, correction: Correction) -> Option<TouchKind> {
    press.counted(correction).map(TouchKind::from)
}

/// Sends whatever keystrokes are queued.
fn flush(outbox: StoredValue<Vec<Touch>>) {
    let batch = outbox.with_value(|queue| queue.clone());
    if batch.is_empty() {
        return;
    }
    if socket::send(&ClientMessage::Touches { touches: batch }) {
        outbox.set_value(Vec::new());
    }
    // If the send failed the batch stays queued; the run is simply unranked.
}

/// The text being typed, coloured per character.
#[component]
fn TypingSurface(
    typist: RwSignal<Option<Typist>>,
    tick: RwSignal<u32>,
    phase: RwSignal<Phase>,
) -> impl IntoView {
    let settings = expect_context::<Settings>();
    // The character list is fixed for the run; only the colours change, so this
    // recomputes when a new exercise arrives rather than on every keystroke.
    let characters = Memo::new(move |_| {
        typist
            .read()
            .as_ref()
            .map(|t| t.text().to_vec())
            .unwrap_or_default()
    });

    view! {
        <div class="surface" tabindex="0">
            {move || {
                (phase.get() == Phase::Ready)
                    .then(|| {
                        view! {
                            <p class="surface-prompt">
                                {move || settings.locale.get().text(Msg::StartTyping)}
                            </p>
                        }
                    })
            }}
            <p class="text">
                {move || {
                    let chars = characters.get();
                    chars
                        .iter()
                        .enumerate()
                        .map(|(index, &ch)| {
                            let state = Memo::new(move |_| {
                                tick.track();
                                typist
                                    .read()
                                    .as_ref()
                                    .map(|t| {
                                        (
                                            t.states().get(index).copied().unwrap_or(CharState::Untouched),
                                            t.cursor() == index,
                                        )
                                    })
                                    .unwrap_or((CharState::Untouched, false))
                            });
                            let class = move || {
                                let (state, at_cursor) = state.get();
                                let name = match state {
                                    CharState::Untouched => "ch",
                                    CharState::Correct => "ch ch-correct",
                                    CharState::Wrong => "ch ch-wrong",
                                    CharState::Retouched => "ch ch-retouched",
                                };
                                if at_cursor {
                                    format!("{name} ch-cursor")
                                } else {
                                    name.to_owned()
                                }
                            };
                            // A newline is shown as a pilcrow followed by a break,
                            // so the typist can see a Return is wanted.
                            if ch == '\n' {
                                view! {
                                    <>
                                        <span class=class>{LINE_END_MARK.to_string()}</span>
                                        <br />
                                    </>
                                }
                                    .into_any()
                            } else {
                                view! { <span class=class>{ch.to_string()}</span> }.into_any()
                            }
                        })
                        .collect_view()
                }}
            </p>
        </div>
    }
}

/// Accuracy, speed and errors while typing.
#[component]
fn LiveStats(typist: RwSignal<Option<Typist>>, tick: RwSignal<u32>) -> impl IntoView {
    let settings = expect_context::<Settings>();
    let current = Memo::new(move |_| {
        tick.track();
        typist.read().as_ref().map(|t| t.score())
    });
    let locale = move || settings.locale.get();

    view! {
        <dl class="stats">
            <div class="stat">
                <dt>{move || locale().text(Msg::Accuracy)}</dt>
                <dd>
                    {move || {
                        current.get().map(|s| format!("{:.0}%", s.accuracy)).unwrap_or_else(|| "—".into())
                    }}
                </dd>
            </div>
            <div class="stat">
                <dt>{move || locale().text(Msg::Speed)}</dt>
                <dd>
                    {move || {
                        current.get().map(|s| format!("{:.0}", s.speed)).unwrap_or_else(|| "—".into())
                    }}
                    <span class="unit">"wpm"</span>
                </dd>
            </div>
            <div class="stat">
                <dt>{move || locale().text(Msg::Errors)}</dt>
                <dd>
                    {move || current.get().map(|s| s.errors.to_string()).unwrap_or_else(|| "—".into())}
                </dd>
            </div>
            <div class="stat">
                <dt>{move || locale().text(Msg::Time)}</dt>
                <dd>
                    {move || {
                        current.get().map(|s| format_duration(s.seconds)).unwrap_or_else(|| "—".into())
                    }}
                </dd>
            </div>
        </dl>
    }
}

/// The panel shown once an exercise is finished.
#[component]
fn Results(
    score: RwSignal<Option<Score>>,
    publishable: RwSignal<bool>,
    would_rank: RwSignal<Option<u32>>,
    published: RwSignal<bool>,
    nickname: RwSignal<String>,
    personal_best: RwSignal<bool>,
) -> impl IntoView {
    let settings = expect_context::<Settings>();
    let progress = expect_context::<ProgressStore>();
    let locale = move || settings.locale.get();
    let goals_met = Memo::new(move |_| {
        score
            .get()
            .is_some_and(|s| settings.module.get().goals().met_by(&s))
    });

    let publish = move |_| {
        socket::send(&ClientMessage::Publish {
            nickname: nickname.get(),
        });
        published.set(true);
    };

    view! {
        <div class="results">
            <h2>{move || locale().text(Msg::Results)}</h2>
            {move || {
                score
                    .get()
                    .map(|s| {
                        view! {
                            <dl class="stats stats-final">
                                <div class="stat">
                                    <dt>{move || locale().text(Msg::Accuracy)}</dt>
                                    <dd>{format!("{:.1}%", s.accuracy)}</dd>
                                </div>
                                <div class="stat">
                                    <dt>{move || locale().text(Msg::Speed)}</dt>
                                    <dd>
                                        {format!("{:.1}", s.speed)}<span class="unit">"wpm"</span>
                                    </dd>
                                </div>
                                {s
                                    .fluidness
                                    .map(|f| {
                                        view! {
                                            <div class="stat">
                                                <dt>{move || locale().text(Msg::Fluidness)}</dt>
                                                <dd>{format!("{f:.1}%")}</dd>
                                            </div>
                                        }
                                    })}
                                <div class="stat">
                                    <dt>{move || locale().text(Msg::Time)}</dt>
                                    <dd>{format_duration(s.seconds)}</dd>
                                </div>
                            </dl>
                        }
                    })
            }}

            <p class=move || {
                if goals_met.get() { "verdict verdict-met" } else { "verdict verdict-missed" }
            }>
                {move || {
                    if goals_met.get() {
                        locale().text(Msg::GoalMet)
                    } else {
                        locale().text(Msg::GoalMissed)
                    }
                }}
                " "
                <span class="band">
                    {move || score.get().map(|s| typing_core::goals::speed_band(s.speed)).unwrap_or("")}
                </span>
            </p>

            {move || {
                personal_best
                    .get()
                    .then(|| {
                        view! {
                            <p class="verdict verdict-met">
                                {move || locale().text(Msg::NewPersonalBest)}
                            </p>
                        }
                    })
            }}
            {move || {
                let module = settings.module.get();
                progress
                    .data
                    .read()
                    .best_for(module)
                    .map(|best| {
                        view! {
                            <p class="notice">
                                {move || locale().text(Msg::PersonalBest)}": "
                                <strong>{format!("{:.1}", best.speed)}</strong>
                                " wpm at "{format!("{:.1}%", best.accuracy)}
                            </p>
                        }
                    })
            }}

            {move || {
                // Cleared a Basic lesson: offer the obvious next step rather
                // than making the visitor go back to the slider.
                let lesson = settings.lesson.get();
                let more_lessons = lesson.lt(&LAST_LESSON);
                (settings.module.get() == Module::Basic && goals_met.get() && more_lessons)
                    .then(|| {
                        view! {
                            <button
                                class="button button-primary"
                                on:click=move |_| settings.lesson.set(lesson + 1)
                            >
                                {move || locale().text(Msg::NextLesson)}" \u{2192}"
                            </button>
                        }
                    })
            }}

            {move || {
                if published.get() {
                    view! { <p class="notice">{move || locale().text(Msg::Leaderboard)}</p> }
                        .into_any()
                } else if publishable.get() {
                    view! {
                        <div class="publish">
                            {move || {
                                would_rank
                                    .get()
                                    .map(|rank| view! { <p class="rank-preview">"#"{rank}</p> })
                            }}
                            <label>
                                {move || locale().text(Msg::Nickname)}
                                <input
                                    type="text"
                                    maxlength=crate::protocol::MAX_NICKNAME_LEN.to_string()
                                    prop:value=move || nickname.get()
                                    on:input=move |ev| nickname.set(event_target_value(&ev))
                                />
                            </label>
                            <button
                                class="button"
                                disabled=move || nickname.get().trim().is_empty()
                                on:click=publish
                            >
                                {move || locale().text(Msg::Publish)}
                            </button>
                        </div>
                    }
                        .into_any()
                } else {
                    view! {
                        <p class="notice">
                            {move || locale().text(Msg::NotPublishable)}
                            " "
                            {move || locale().text(Msg::PublishRules)}
                        </p>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// The module tabs, plus the lesson picker when Basic is showing.
#[component]
fn ModulePicker() -> impl IntoView {
    let settings = expect_context::<Settings>();
    let progress = expect_context::<ProgressStore>();
    let locale = move || settings.locale.get();

    // Hoisted out of the markup on purpose. The `view!` macro scans for `>` to
    // find the end of a tag, so a `>=` inside an attribute expression closes the
    // tag early and the rest of the Rust silently becomes text content — which
    // is exactly what happened here before these were memos.
    let at_first_lesson = Memo::new(move |_| settings.lesson.get() <= 1);
    let at_last_lesson = Memo::new(move |_| settings.lesson.get() >= LAST_LESSON);
    let lessons_cleared = Memo::new(move |_| progress.data.read().lesson_reached);

    view! {
        <nav class="modules" aria-label="Practice modules">
            {Module::ALL
                .into_iter()
                .map(|module| {
                    let class = move || {
                        if settings.module.get() == module {
                            "module module-current"
                        } else {
                            "module"
                        }
                    };
                    view! {
                        <button
                            class=class
                            aria-current=move || {
                                (settings.module.get() == module).then_some("page")
                            }
                            on:click=move |_| settings.module.set(module)
                        >
                            <span class="module-name">{move || locale().module_name(module)}</span>
                            <span class="module-blurb">{move || locale().module_blurb(module)}</span>
                        </button>
                    }
                })
                .collect_view()}
        </nav>

        {move || {
            (settings.module.get() == Module::Basic)
                .then(|| {
                    view! {
                        <div class="lesson-picker">
                            <label for="lesson-slider">
                                {move || locale().text(Msg::Lesson)}
                            </label>
                            <button
                                class="button button-step"
                                title=move || locale().text(Msg::PreviousLesson)
                                aria-label=move || locale().text(Msg::PreviousLesson)
                                disabled=at_first_lesson
                                on:click=move |_| {
                                    settings.lesson.update(|n| *n = n.saturating_sub(1).max(1))
                                }
                            >
                                "\u{2039}"
                            </button>
                            <input
                                id="lesson-slider"
                                type="range"
                                min="1"
                                max=LAST_LESSON.to_string()
                                prop:value=move || settings.lesson.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(n) = event_target_value(&ev).parse::<u32>() {
                                        settings.lesson.set(n.clamp(1, LAST_LESSON));
                                    }
                                }
                            />
                            <output>{move || settings.lesson.get()}</output>
                            <button
                                class="button button-step"
                                title=move || locale().text(Msg::NextLesson)
                                aria-label=move || locale().text(Msg::NextLesson)
                                disabled=at_last_lesson
                                on:click=move |_| {
                                    settings.lesson.update(|n| *n = (*n + 1).min(LAST_LESSON))
                                }
                            >
                                "\u{203a}"
                            </button>
                            {move || {
                                let reached = lessons_cleared.get();
                                (reached != 0)
                                    .then(|| {
                                        view! {
                                            <span class="lesson-reached">
                                                "cleared up to "{reached}
                                            </span>
                                        }
                                    })
                            }}
                        </div>
                    }
                })
        }}
    }
}

/// Formats seconds as `m:ss`.
fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Fetches practice text for a language into `into`.
#[cfg(feature = "hydrate")]
fn fetch_corpus(language: String, into: RwSignal<Option<Corpus>>) {
    leptos::task::spawn_local(async move {
        let url = format!("/api/corpus/{language}");
        match gloo_net::http::Request::get(&url).send().await {
            Ok(response) if response.ok() => match response.json::<Corpus>().await {
                Ok(corpus) => into.set(Some(corpus)),
                Err(error) => {
                    web_sys::console::warn_1(&format!("corpus {language}: {error}").into())
                }
            },
            Ok(response) => {
                web_sys::console::warn_1(
                    &format!("corpus {language}: HTTP {}", response.status()).into(),
                );
            }
            Err(error) => {
                web_sys::console::warn_1(&format!("corpus {language}: {error}").into());
            }
        }
    });
}

/// No fetching without a browser.
#[cfg(not(feature = "hydrate"))]
fn fetch_corpus(_language: String, _into: RwSignal<Option<Corpus>>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_minutes_and_seconds() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(9.4), "0:09");
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(600.0), "10:00");
        assert_eq!(format_duration(-5.0), "0:00", "never negative");
    }

    #[test]
    fn a_wrong_key_is_reported_differently_in_each_mode() {
        assert_eq!(
            touch_kind(Press::Wrong, Correction::Forbidden),
            Some(TouchKind::Wrong),
            "going forward, a mistake is a counted keystroke"
        );
        assert_eq!(
            touch_kind(Press::Wrong, Correction::Required),
            Some(TouchKind::Stumble),
            "in correction mode it counts for nothing until retyped"
        );
        assert_eq!(touch_kind(Press::Ignored, Correction::Forbidden), None);
    }

    #[test]
    fn a_full_local_run_produces_a_reportable_stream() {
        // What the keydown handler does, without a browser: type an exercise and
        // check the resulting stream is what the server expects to verify.
        let layout = load_layout("qwerty_us").unwrap();
        let lessons = klavaro_lessons();
        let generated = exercise::generate(
            exercise::Request {
                module: Module::Basic,
                layout: &layout,
                lesson: Some(&lessons[0]),
                corpus: None,
                stop_marks: true,
            },
            7,
        )
        .unwrap();

        let mut t = Typist::for_module(&generated.text, Module::Basic);
        let mut stream = Vec::new();
        let mut at_us = 0u64;
        for ch in generated.text.chars() {
            at_us += 120_000;
            let press = t.press(Key::Char(ch), at_us);
            if let Some(kind) = touch_kind(press, Correction::Forbidden) {
                stream.push(Touch {
                    kind,
                    dt_us: 120_000,
                });
            }
        }
        assert!(t.is_finished());
        assert_eq!(
            stream.len(),
            generated.len_chars(),
            "one report per character"
        );

        let replayed = crate::protocol::replay(&stream);
        assert_eq!(replayed.touches, t.session().touches);
        assert_eq!(replayed.errors, t.session().errors);
        assert_eq!(
            replayed.score(),
            t.score(),
            "server would score it the same"
        );
    }

    #[test]
    fn correction_runs_report_stumbles_that_do_not_count() {
        let mut t = Typist::new("fjfj", Correction::Required);
        let mut stream = Vec::new();
        let mut push = |press: Press| {
            if let Some(kind) = touch_kind(press, Correction::Required) {
                stream.push(Touch {
                    kind,
                    dt_us: 100_000,
                });
            }
        };
        push(t.press(Key::Char('f'), 100_000));
        push(t.press(Key::Char('z'), 200_000));
        push(t.press(Key::Backspace, 300_000));
        push(t.press(Key::Char('j'), 400_000));
        push(t.press(Key::Char('f'), 500_000));
        push(t.press(Key::Char('j'), 600_000));

        let replayed = crate::protocol::replay(&stream);
        assert_eq!(
            replayed.touches, 4,
            "four positions, four counted keystrokes"
        );
        assert_eq!(replayed.errors, 1);
        assert_eq!(replayed.touches, t.session().touches);
        assert_eq!(replayed.errors, t.session().errors);
    }
}
