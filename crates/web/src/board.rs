//! The shared top-10 table.

use leptos::prelude::*;

use crate::{i18n::Msg, protocol::BoardEntry, settings::Settings};

/// Renders a leaderboard, or a nudge when it is empty.
#[component]
pub fn BoardTable(
    /// The rows to show, best first.
    entries: RwSignal<Vec<BoardEntry>>,
) -> impl IntoView {
    let settings = expect_context::<Settings>();
    let locale = move || settings.locale.get();

    view! {
        <section class="board">
            <h2>{move || locale().text(Msg::Leaderboard)}</h2>
            {move || {
                if entries.read().is_empty() {
                    view! { <p class="notice">{move || locale().text(Msg::BoardEmpty)}</p> }
                        .into_any()
                } else {
                    view! {
                        <table>
                            <thead>
                                <tr>
                                    <th scope="col">{move || locale().text(Msg::Rank)}</th>
                                    <th scope="col">{move || locale().text(Msg::Nickname)}</th>
                                    <th scope="col">{move || locale().text(Msg::Speed)}</th>
                                    <th scope="col">{move || locale().text(Msg::Accuracy)}</th>
                                    <th scope="col">{move || locale().text(Msg::Fluidness)}</th>
                                    <th scope="col">{move || locale().text(Msg::AchievedOn)}</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For
                                    each=move || entries.get()
                                    key=|entry| (entry.rank, entry.nickname.clone())
                                    let:entry
                                >
                                    <tr>
                                        <td class="rank">{entry.rank}</td>
                                        <td>{entry.nickname.clone()}</td>
                                        <td class="number">{format!("{:.1}", entry.speed)}</td>
                                        <td class="number">{format!("{:.1}%", entry.accuracy)}</td>
                                        <td class="number">
                                            {entry
                                                .fluidness
                                                .map(|f| format!("{f:.1}%"))
                                                .unwrap_or_else(|| "—".into())}
                                        </td>
                                        <td class="date">{entry.achieved_on.clone()}</td>
                                    </tr>
                                </For>
                            </tbody>
                        </table>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
