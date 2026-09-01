//! Credits, licence and a plain account of what the hosted version stores.

use leptos::prelude::*;

/// The about page.
#[component]
pub fn About() -> impl IntoView {
    view! {
        <section class="prose">
            <h1>"About Tinkhaven Typing"</h1>

            <p>
                "Tinkhaven Typing is a web port of "
                <a href="https://klavaro.sourceforge.io/" rel="noreferrer">"Klavaro"</a>
                ", the touch typing tutor by Felipe Emmanuel Ferreira de Castro. Klavaro is "
                "© 2005–2021 Felipe Emmanuel Ferreira de Castro and is published under the "
                "GNU General Public Licence, version 3 or later."
            </p>
            <p>
                "This port reuses Klavaro's keyboard layouts, its 43 progressive lessons, its "
                "practice corpora and its scoring formulas, so a score here means the same thing "
                "it means in the desktop program. It is "<strong>"not"</strong>
                " affiliated with or endorsed by the Klavaro project — please report problems "
                "with this port here rather than upstream."
            </p>

            <h2>"Licence"</h2>
            <p>
                "Because it is a derivative work, Tinkhaven Typing is itself distributed under "
                "the GNU GPL version 3 or later. The page you are reading runs compiled "
                "WebAssembly, which is a distribution of object code, so you are entitled to the "
                "corresponding source — it is at "
                <a href="https://github.com/tinkhaven/typing" rel="noreferrer">
                    "github.com/tinkhaven/typing"
                </a>"."
            </p>

            <h2>"What this site stores"</h2>
            <p>
                "There are no accounts and no cookies. Your keyboard, language and lesson "
                "choices are kept in your own browser's local storage and never sent anywhere. "
                "Nothing at all is recorded about you unless you choose to publish a score, and "
                "then only the nickname you type, three numbers and the date — no email, no IP "
                "address. Published rows are deleted automatically after a year."
            </p>
            <p>
                "While you practise, the page sends the server a stream of keystroke outcomes — "
                "whether each keystroke was right and how long after the previous one it landed. "
                "That is what lets the server score a run for the leaderboard rather than take "
                "the browser's word for it. It is not stored unless you publish."
            </p>

            <h2>"How the numbers are worked out"</h2>
            <ul>
                <li>
                    <strong>"Accuracy"</strong>
                    " is the share of keystrokes that matched what was asked for."
                </li>
                <li>
                    <strong>"Speed"</strong>
                    " counts a word as five characters, and counts only correct keystrokes."
                </li>
                <li>
                    <strong>"Fluidness"</strong>
                    " measures how even your rhythm is. It rewards a steady pace rather than "
                    "punishing the occasional pause, so a single hesitation barely moves it."
                </li>
            </ul>
            <p class="caveat">
                "The leaderboard is scored by the server rather than the browser, which keeps "
                "out accidents and casual tampering. It is not proof against someone determined "
                "to fake a run, and it is not meant to be — it is a typing tutor."
            </p>
        </section>
    }
}
