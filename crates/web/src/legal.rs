//! Privacy policy and terms.
//!
//! Written against what the deployment actually does, not from a template. Every
//! factual claim here was checked against the running system: the load
//! balancer's access logging is off, the container logs carry no visitor IP
//! addresses, and the only thing written to the database is what a visitor
//! chooses to publish.
//!
//! Keep it that way. If a change starts collecting something — an analytics
//! script, a cookie, an IP in a log line — this page is part of the change, and
//! a policy that overstates the restraint is worse than no policy at all.

use leptos::prelude::*;

/// The operator of this deployment.
///
/// GDPR Article 13(1)(a) requires the controller's identity and contact details,
/// so these must be real before the site can be said to have a lawful privacy
/// notice. They are placeholders in the repository because only the operator can
/// supply them; [`operator_details_published`] reports whether they still are,
/// and the page says so plainly rather than pretending.
pub const OPERATOR_NAME: &str = "REPLACE ME: legal entity or individual's name";
/// Postal address of the operator.
pub const OPERATOR_ADDRESS: &str = "REPLACE ME: postal address";
/// Where a visitor can exercise their rights.
pub const OPERATOR_EMAIL: &str = "REPLACE ME: contact email address";

/// Whether the operator has filled in who they are.
pub fn operator_details_published() -> bool {
    ![OPERATOR_NAME, OPERATOR_ADDRESS, OPERATOR_EMAIL]
        .iter()
        .any(|value| value.starts_with("REPLACE ME"))
}

/// Shown while the operator details are still placeholders.
#[component]
fn UnconfiguredNotice() -> impl IntoView {
    (!operator_details_published()).then(|| {
        view! {
            <p class="notice notice-problem">
                <strong>"This deployment has not published its operator details."</strong>
                " Until the name, address and contact address below are filled in, this
                page does not meet the identification requirement in Article 13 of the
                GDPR. If you are running this, edit "
                <code>"crates/web/src/legal.rs"</code>"."
            </p>
        }
    })
}

/// The privacy policy.
#[component]
pub fn Privacy() -> impl IntoView {
    view! {
        <section class="prose">
            <h1>"Privacy"</h1>

            <UnconfiguredNotice />

            <p class="lede">
                "The short version: no analytics, no tracking, and nothing stored about
                you unless you ask for it. Your settings and scores live in your own
                browser. Two things are optional and deliberate: publishing a score to
                the leaderboard, and signing in to carry progress between devices. Signing
                in sets one cookie and nothing else; without it there are no cookies at
                all."
            </p>

            <h2>"Who is responsible"</h2>
            <p>
                {OPERATOR_NAME}", "{OPERATOR_ADDRESS}". For anything on this page, "
                "including a request to exercise your rights, write to "
                <strong>{OPERATOR_EMAIL}</strong>"."
            </p>

            <h2>"What happens when you just use the site"</h2>
            <p>
                "Nothing is recorded. No cookie is set, no identifier is assigned, and no
                analytics or tracking script runs — there are none on the page at all.
                That remains true for as long as you do not sign in."
            </p>
            <p>
                "Your keyboard layout, interface language, practice language, current
                module and lesson, along with your best score per module, how many
                exercises you have finished and the highest Basic lesson you have
                cleared, are stored in your browser's own local storage. That data stays
                on your device. It is not sent to the server, and the operator cannot
                see it. Clearing your browser's site data for this domain deletes it."
            </p>

            <h2>"If you sign in"</h2>
            <p>
                "Signing in is optional and exists for one reason: to carry your progress
                between devices. Everything works without it."
            </p>
            <p>
                "Sign-in uses Google. The only thing asked of Google is the "
                <code>"openid"</code>" scope — deliberately not "<code>"email"</code>
                " and not "<code>"profile"</code>". Google therefore returns a
                pseudonymous subject identifier and nothing else. "
                <strong>
                    "This site never receives your email address or your name, and
                    cannot contact or identify you."
                </strong>
            </p>
            <p>
                "That identifier is not stored either. What is stored is a keyed hash of
                it, so the record cannot be matched back to a Google account without a
                secret held only by the server. Against that hash sits exactly the same
                progress your browser already keeps: best score per module, how many
                exercises you have finished, and how far through the Basic lessons you
                have got."
            </p>
            <p>
                "One cookie is set, holding a signed session — no tracking identifier and
                nothing readable by anyone else. It is strictly necessary for signing in,
                which is why there is no consent banner asking about it; if you do not
                sign in, it is never set. It lasts 90 days and is removed when you sign
                out. Profiles nobody has touched for two years are deleted automatically."
            </p>
            <p>
                "The legal basis is your consent, given by choosing to sign in. There is
                a "<strong>"Delete my data"</strong>" button next to the sign-out link
                that erases the stored profile immediately, without asking anyone."
            </p>
            <p>
                "Google's own handling of your sign-in — that you authenticated, and when
                — is Google's processing under its own privacy policy, not something this
                site controls or can see."
            </p>
            <p>
                "The consequence of holding so little is worth stating plainly: "
                <strong>"there is no account recovery"</strong>
                ". If you lose access to your Google account, the progress behind it
                cannot be reached by you, by us, or by anybody — because there is nothing
                stored that could prove it was yours."
            </p>

            <h2>"What crosses the network while you type"</h2>
            <p>
                "Keystrokes are judged in your browser, not on the server. While an
                exercise is running, the page sends the server a stream of outcomes:
                for each keystroke, whether it matched what was asked for and how long
                after the previous one it arrived. It never sends which key you pressed."
            </p>
            <p>
                "This exists so the server can score a run itself rather than take the
                browser's word for it, which is what makes a shared leaderboard mean
                anything. The stream is held in memory for the duration of the exercise
                and then discarded. It is not written to disk and not linked to any
                identifier."
            </p>

            <h2>"If you publish a score"</h2>
            <p>
                "Publishing is a deliberate act: you type a nickname and press a button.
                Doing so records the nickname you chose, your speed, accuracy and
                fluidness, and the date — and nothing else. No email address, no account,
                no IP address."
            </p>
            <p>
                "The nickname is whatever you type. It does not have to be, and should
                not be, your name. Entries are deleted automatically one year after they
                are set."
            </p>
            <p>
                "The legal basis is your consent, given by pressing the button. You can
                withdraw it by asking for the entry to be removed."
            </p>

            <h2>"Logs"</h2>
            <p>
                "The server writes operational logs — that it started, that it loaded its
                practice text, and any errors. Load-balancer access logging is switched
                off and visitor IP addresses are not written to the application log.
                Amazon Web Services, as the hosting provider, processes the connection
                itself in order to route it; that is inherent to serving a web page over
                the internet."
            </p>

            <h2>"Where the data is"</h2>
            <p>
                "Everything runs in Amazon Web Services' Ireland region (eu-west-1) and
                stays in the European Union. AWS acts as a processor on the operator's
                behalf. There are no other processors, no third-party embeds and no
                content delivery network in front of the site."
            </p>

            <h2>"Your rights"</h2>
            <p>
                "Under the GDPR you may ask for access to your personal data, and for its
                correction, erasure or restriction; you may object to processing; and you
                may ask for it in a portable form. Write to the address above."
            </p>
            <p>
                "In practice there is little to ask about, because little is held. If you
                are signed in, the "<strong>"Delete my data"</strong>" button does the
                whole of access-and-erasure for your profile without involving anybody —
                it is your data and it should not need a letter."
            </p>
            <p>
                "A published leaderboard entry is identified solely by a nickname you
                invented, so please quote the nickname and roughly when you set it. That
                also means the operator has no way to verify a nickname is yours; a
                request to remove one will be honoured anyway, which is the trade-off that
                keeping no identity brings."
            </p>
            <p>
                "You can also complain to a supervisory authority. In Belgium that is the
                Data Protection Authority, "
                <a href="https://www.dataprotectionauthority.be/" rel="noreferrer">
                    "dataprotectionauthority.be"
                </a>"."
            </p>

            <h2>"Changes"</h2>
            <p>
                "This site is open source, so changes to what it does are visible in its
                history. If it starts collecting something it does not collect today,
                this page changes in the same commit."
            </p>
        </section>
    }
}

/// The terms of use.
#[component]
pub fn Terms() -> impl IntoView {
    view! {
        <section class="prose">
            <h1>"Terms of use"</h1>

            <UnconfiguredNotice />

            <h2>"What this is"</h2>
            <p>
                "A free touch-typing tutor, offered as-is by "{OPERATOR_NAME}". It is a
                hobby deployment of an open-source program, not a commercial service.
                There is no uptime commitment, no warranty, and no guarantee that scores
                or leaderboard entries will survive — the software is provided without
                warranty of any kind, as the GNU General Public Licence sets out."
            </p>

            <h2>"The software"</h2>
            <p>
                "Tinkhaven Typing is a web port of "
                <a href="https://klavaro.sourceforge.io/" rel="noreferrer">"Klavaro"</a>
                " by Felipe Emmanuel Ferreira de Castro, and is distributed under the GNU
                General Public Licence, version 3 or later. You are free to use, study,
                share and modify it. The source is at "
                <a href="https://github.com/tinkhaven/typing" rel="noreferrer">
                    "github.com/tinkhaven/typing"
                </a>", which is also where the corresponding source for the WebAssembly
                this page runs can be found."
            </p>

            <h2>"The leaderboard"</h2>
            <p>
                "Please pick a nickname that is not offensive and is not somebody else's
                name or personal data. Entries that are may be removed without notice.
                Deliberately fabricating results spoils the board for everyone; the server
                scores runs itself and rejects impossible ones, but it is not proof
                against someone determined, and it does not try to be."
            </p>

            <h2>"Liability"</h2>
            <p>
                "To the extent the law allows, the operator is not liable for any loss
                arising from use of this site. Nothing here limits rights you have as a
                consumer that cannot be limited by agreement."
            </p>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_check_notices_unfilled_details() {
        // The whole point of the notice is that it cannot be forgotten, so this
        // guards the detection rather than the constants themselves.
        assert!(
            !operator_details_published(),
            "the repository ships placeholders; if this fails, someone filled them \
             in and should update this test to assert the opposite"
        );
    }
}
