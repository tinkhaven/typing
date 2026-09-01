//! The WebSocket endpoint.
//!
//! One connection is one typist's practice session. The server issues seeds,
//! keeps its own tally of the keystrokes it is told about, scores what it
//! counted rather than what it was told, and pushes board changes as they
//! happen. See [`crate::protocol`] for why the typing loop itself is not here.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use typing_core::{
    exercise,
    goals::Module,
    lesson::Lesson,
    load_layout,
    stats::Session,
    typist::Correction,
};

use crate::protocol::{
    apply_all, clean_nickname, ClientMessage, ProblemCode, ServerMessage, Touch,
};

use super::{
    leaderboard::{Submission},
    verify::{verify, Expectation},
    AppState, BoardChanged,
};

/// Messages queued to one client. Small: a board is ten rows.
const SEND_QUEUE: usize = 32;

/// Upgrades an HTTP request to a practice session.
pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| serve(socket, state))
}

/// What the connection is currently working on.
struct Active {
    module: Module,
    language: String,
    expectation: Expectation,
    /// The server's own tally, built from the keystrokes reported so far.
    session: Session,
    /// Microseconds into the session, tracking the reported gaps.
    at_us: u64,
}

/// A scored result waiting for the visitor to decide whether to publish it.
struct Pending {
    module: Module,
    language: String,
    speed: f64,
    accuracy: f64,
    fluidness: Option<f64>,
}

async fn serve(socket: WebSocket, state: AppState) {
    let (mut sink, mut stream) = socket.split();
    let (outbox, mut queued) = mpsc::channel::<ServerMessage>(SEND_QUEUE);

    // One task owns the write half, so both the request loop and the board
    // subscription can send without contending for the socket.
    let writer = tokio::spawn(async move {
        while let Some(message) = queued.recv().await {
            let Ok(json) = serde_json::to_string(&message) else {
                tracing::error!("could not serialise a server message");
                continue;
            };
            if sink.send(Message::Text(json.into())).await.is_err() {
                break; // client went away
            }
        }
        let _ = sink.close().await;
    });

    let mut active: Option<Active> = None;
    let mut pending: Option<Pending> = None;
    let mut watching: Option<(Module, String)> = None;
    let mut board_updates = state.board_changes.subscribe();

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    // Ping/Pong are handled by axum; binary frames are not used.
                    _ => continue,
                };
                let Ok(request) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
                    let _ = outbox.send(problem(
                        ProblemCode::OutOfOrder,
                        "could not read that message",
                    )).await;
                    continue;
                };
                let replies = handle(
                    request,
                    &state,
                    &mut active,
                    &mut pending,
                    &mut watching,
                ).await;
                for reply in replies {
                    if outbox.send(reply).await.is_err() {
                        break;
                    }
                }
            }
            changed = board_updates.recv() => {
                let Ok(BoardChanged { module, language }) = changed else { continue };
                // Only push a board this connection actually asked to follow.
                if watching.as_ref() != Some(&(module, language.clone())) {
                    continue;
                }
                if let Ok(entries) = state.leaderboard.top(module, &language).await {
                    let _ = outbox.send(ServerMessage::Board { module, language, entries }).await;
                }
            }
        }
    }

    drop(outbox);
    let _ = writer.await;
}

/// Handles one request, returning what to send back.
async fn handle(
    request: ClientMessage,
    state: &AppState,
    active: &mut Option<Active>,
    pending: &mut Option<Pending>,
    watching: &mut Option<(Module, String)>,
) -> Vec<ServerMessage> {
    match request {
        ClientMessage::Ping => vec![ServerMessage::Pong],

        ClientMessage::Start { module, layout, language, lesson, seed } => {
            match start(state, module, &layout, &language, lesson, seed) {
                Ok((started, reply)) => {
                    *active = Some(started);
                    *pending = None;
                    vec![reply]
                }
                Err(reply) => vec![reply],
            }
        }

        ClientMessage::Touches { touches } => {
            let Some(current) = active.as_mut() else {
                return vec![problem(
                    ProblemCode::OutOfOrder,
                    "keystrokes arrived before an exercise was started",
                )];
            };
            accumulate(current, &touches);
            // Nothing to reply: the client already renders its own live figures.
            vec![]
        }

        ClientMessage::Finish { client_session } => {
            let Some(current) = active.as_ref() else {
                return vec![problem(
                    ProblemCode::OutOfOrder,
                    "no exercise was in progress",
                )];
            };

            if let Err(rejection) = verify(&current.session, &current.expectation, true) {
                tracing::info!(%rejection, "rejected a reported session");
                *active = None;
                return vec![problem(ProblemCode::Implausible, rejection.to_string())];
            }

            let score = current.session.score();
            // Divergence between the two tallies means a lost batch or a broken
            // client. The server's count is the one that counts; log the gap so
            // it does not go unnoticed.
            if client_session.touches != current.session.touches {
                tracing::warn!(
                    client = client_session.touches,
                    server = current.session.touches,
                    "keystroke tallies disagree",
                );
            }

            let goals_met = current.module.goals().met_by(&score);
            let publishable = current.module.is_ranked()
                && goals_met
                && score.touches >= current.module.min_chars_to_rank();

            let would_rank = if publishable {
                state
                    .leaderboard
                    .top(current.module, &current.language)
                    .await
                    .ok()
                    .map(|board| {
                        let ahead = board.iter().filter(|e| e.speed > score.speed).count();
                        ahead as u32 + 1
                    })
            } else {
                None
            };

            *pending = publishable.then(|| Pending {
                module: current.module,
                language: current.language.clone(),
                speed: score.speed,
                accuracy: score.accuracy,
                fluidness: score.fluidness,
            });
            *active = None;

            vec![ServerMessage::Scored { score, goals_met, publishable, would_rank }]
        }

        ClientMessage::Publish { nickname } => {
            let Some(result) = pending.take() else {
                return vec![problem(
                    ProblemCode::OutOfOrder,
                    "there is no result to publish",
                )];
            };
            let Some(nickname) = clean_nickname(&nickname) else {
                *pending = Some(result); // keep it; the visitor can try again
                return vec![problem(ProblemCode::BadNickname, "please choose a name")];
            };

            let submission = Submission {
                module: result.module,
                language: result.language.clone(),
                nickname,
                speed: result.speed,
                accuracy: result.accuracy,
                fluidness: result.fluidness,
            };
            match state.leaderboard.submit(submission).await {
                Ok(_) => {
                    let _ = state.board_changes.send(BoardChanged {
                        module: result.module,
                        language: result.language.clone(),
                    });
                    board_reply(state, result.module, &result.language).await
                }
                Err(error) => {
                    tracing::error!(%error, "could not record a result");
                    vec![problem(
                        ProblemCode::StoreUnavailable,
                        "the leaderboard is unavailable right now",
                    )]
                }
            }
        }

        ClientMessage::WatchBoard { module, language } => {
            *watching = Some((module, language.clone()));
            board_reply(state, module, &language).await
        }
    }
}

/// Validates a start request and issues a seed.
fn start(
    state: &AppState,
    module: Module,
    layout_name: &str,
    language: &str,
    lesson_number: Option<u32>,
    seed: u64,
) -> Result<(Active, ServerMessage), ServerMessage> {
    let Some(layout) = load_layout(layout_name) else {
        return Err(problem(
            ProblemCode::UnknownLayout,
            format!("no keyboard layout called {layout_name}"),
        ));
    };

    let lesson: Option<&Lesson> = if module == Module::Basic {
        let number = lesson_number.unwrap_or(1);
        let found = state
            .lessons
            .iter()
            .find(|l| l.number == number)
            .ok_or_else(|| {
                problem(
                    ProblemCode::UnknownLesson,
                    format!("there is no lesson {number}"),
                )
            })?;
        Some(found)
    } else {
        None
    };

    let corpus = match module {
        Module::Velocity | Module::Fluidness => {
            let found = state.corpora.get(language).ok_or_else(|| {
                problem(
                    ProblemCode::UnknownLanguage,
                    format!("no practice text for {language}"),
                )
            })?;
            Some(found)
        }
        _ => None,
    };

    // Regenerate the client's exercise so the server knows what was on screen.
    let generated = exercise::generate(
        exercise::Request {
            module,
            layout: &layout,
            lesson,
            corpus: corpus.as_deref(),
            stop_marks: true,
        },
        seed,
    )
    .map_err(|e| problem(ProblemCode::Implausible, e.to_string()))?;

    let expected_chars = generated.len_chars() as u32;
    Ok((
        Active {
            module,
            language: language.to_owned(),
            expectation: Expectation {
                module,
                chars: expected_chars,
                correction: Correction::for_module(module),
            },
            session: Session::new(),
            at_us: 0,
        },
        ServerMessage::Following { expected_chars },
    ))
}

/// Folds a batch of keystrokes into the server's running tally.
///
/// The session continues across batches rather than being rebuilt per batch, so
/// how the client chose to group its keystrokes cannot change the score.
fn accumulate(current: &mut Active, touches: &[Touch]) {
    let mut at_us = current.at_us;
    apply_all(&mut current.session, touches, &mut at_us);
    current.at_us = at_us;
}

async fn board_reply(state: &AppState, module: Module, language: &str) -> Vec<ServerMessage> {
    match state.leaderboard.top(module, language).await {
        Ok(entries) => vec![ServerMessage::Board {
            module,
            language: language.to_owned(),
            entries,
        }],
        Err(error) => {
            tracing::error!(%error, "could not read a leaderboard");
            vec![problem(
                ProblemCode::StoreUnavailable,
                "the leaderboard is unavailable right now",
            )]
        }
    }
}

fn problem(code: ProblemCode, detail: impl Into<String>) -> ServerMessage {
    ServerMessage::Problem { code, detail: detail.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TouchKind;

    fn active() -> Active {
        Active {
            module: Module::Velocity,
            language: "nl".into(),
            expectation: Expectation {
                module: Module::Velocity,
                chars: 100,
                correction: Correction::Forbidden,
            },
            session: Session::new(),
            at_us: 0,
        }
    }

    fn touch(kind: TouchKind, dt_us: u32) -> Touch {
        Touch { kind, dt_us }
    }

    #[test]
    fn batches_accumulate_into_one_continuous_session() {
        let mut current = active();
        accumulate(
            &mut current,
            &[
                touch(TouchKind::Correct, 100_000),
                touch(TouchKind::Correct, 100_000),
            ],
        );
        accumulate(
            &mut current,
            &[
                touch(TouchKind::Correct, 100_000),
                touch(TouchKind::Wrong, 100_000),
            ],
        );
        assert_eq!(current.session.touches, 4);
        assert_eq!(current.session.errors, 1);
        assert_eq!(current.session.elapsed_us, 400_000, "time is continuous");
    }

    #[test]
    fn batching_does_not_change_the_score() {
        // Whatever the client's batch size, the server must see the same session:
        // gaps are recorded between consecutive keystrokes, not within a batch.
        let stream: Vec<Touch> = (0..12).map(|_| touch(TouchKind::Correct, 100_000)).collect();

        let mut whole = active();
        accumulate(&mut whole, &stream);

        let mut one_at_a_time = active();
        for single in &stream {
            accumulate(&mut one_at_a_time, std::slice::from_ref(single));
        }

        let mut in_fives = active();
        for chunk in stream.chunks(5) {
            accumulate(&mut in_fives, chunk);
        }

        assert_eq!(whole.session.intervals_us.len(), 11, "12 keystrokes, 11 gaps");
        assert_eq!(one_at_a_time.session.intervals_us, whole.session.intervals_us);
        assert_eq!(in_fives.session.intervals_us, whole.session.intervals_us);
        assert_eq!(one_at_a_time.session.score(), whole.session.score());
        assert_eq!(in_fives.session.score(), whole.session.score());
    }

    #[test]
    fn accumulated_sessions_still_pass_verification() {
        let mut current = active();
        current.expectation.chars = 4;
        accumulate(
            &mut current,
            &[
                touch(TouchKind::Correct, 200_000),
                touch(TouchKind::Correct, 200_000),
            ],
        );
        accumulate(
            &mut current,
            &[
                touch(TouchKind::Correct, 200_000),
                touch(TouchKind::Correct, 200_000),
            ],
        );
        assert_eq!(verify(&current.session, &current.expectation, true), Ok(()));
    }
}
