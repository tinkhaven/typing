//! Privacy policy and terms of use.
//!
//! Written to match the house style already published at
//! `app.tinkhaven.com/privacy` and `/terms`: short sections, plain language, and
//! the company details kept on the shared legal notice rather than repeated.
//!
//! Every factual claim here was checked against the running system before being
//! written: the load balancer's access logging is off, the container log carries
//! no visitor IP addresses, and nothing reaches the database unless a visitor
//! publishes a score or signs in. Keep it that way. If a change starts
//! collecting something, this page is part of that change — a privacy notice
//! that overstates the restraint is worse than none at all.

use leptos::prelude::*;

/// The controller, as published on the shared legal notice.
pub const CONTROLLER: &str = "Tinkhaven 2.0 BV";

/// Where to write about anything on these pages.
pub const PRIVACY_EMAIL: &str = "privacy@tinkhaven.com";

/// The shared legal notice carrying the full company details.
pub const IMPRINT_URL: &str = "https://app.tinkhaven.com/imprint";

/// Shown under each page title.
pub const LAST_UPDATED: &str = "Last updated: 2026";

/// The privacy policy.
#[component]
pub fn Privacy() -> impl IntoView {
    view! {
        <section class="legal">
            <header class="legal-header">
                <h1>"Privacy Policy"</h1>
                <p class="legal-meta">{LAST_UPDATED}</p>
            </header>

            <div class="at-a-glance">
                <h2>"At a glance"</h2>
                <ul>
                    <li>"Nothing is tracked. No analytics, no advertising, no third-party
                        scripts — there are none on the page at all."</li>
                    <li>"Your settings and your scores are kept in your own browser, not
                        on our servers."</li>
                    <li>"Everything runs in the EU. There is no other processor and no
                        content delivery network in front of the site."</li>
                    <li class="qualified">"Two things are optional and up to you:
                        publishing a score to the leaderboard, and signing in to carry
                        progress between devices. Do neither and this site stores nothing
                        about you."</li>
                    <li class="qualified">"No cookies at all unless you sign in — and then
                        exactly one, to keep you signed in. No consent banner is needed
                        for it."</li>
                </ul>
            </div>

            <h2>"Who we are"</h2>
            <p>
                "The data controller is "{CONTROLLER}" (Belgium). Contact: "
                <a href={format!("mailto:{PRIVACY_EMAIL}")}>{PRIVACY_EMAIL}</a>
                ". Full company details are on our "
                <a href={IMPRINT_URL} rel="noreferrer">"legal notice"</a>"."
            </p>

            <h2>"This website"</h2>
            <p>
                "typing.tinkhaven.com is a touch typing tutor. It uses no analytics or
                trackers and embeds no third-party scripts. It sets no cookies unless you
                sign in, so no cookie consent is required. It is served from AWS data
                centres in Ireland (eu-west-1)."
            </p>
            <p>
                "Your keyboard layout, languages, current lesson and your best scores are
                stored in your browser's own local storage. That data stays on your
                device, is never sent to us, and clearing your browser's site data for
                this domain deletes it."
            </p>

            <h2>"What we process"</h2>
            <table>
                <thead>
                    <tr>
                        <th scope="col">"What"</th>
                        <th scope="col">"When"</th>
                        <th scope="col">"Why, and for how long"</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>"Nothing"</td>
                        <td>"Ordinary use"</td>
                        <td>"No cookie, no identifier, no record."</td>
                    </tr>
                    <tr>
                        <td>"Keystroke outcomes — whether each keystroke was right, and
                            the gap since the previous one. Never which key."</td>
                        <td>"While an exercise is running"</td>
                        <td>"So the server can score a run itself rather than trust the
                            browser's arithmetic, which is what makes a shared
                            leaderboard mean anything. Held in memory for the exercise,
                            then discarded. Necessary to provide the feature you asked
                            for, GDPR Art. 6(1)(b)."</td>
                    </tr>
                    <tr>
                        <td>"A nickname you type, your speed, accuracy, fluidness and the
                            date"</td>
                        <td>"Only if you publish a score"</td>
                        <td>"To show the leaderboard. Consent, Art. 6(1)(a), given by
                            pressing the button. Deleted automatically after one year."</td>
                    </tr>
                    <tr>
                        <td>"A pseudonymous identifier and the same progress your browser
                            already holds"</td>
                        <td>"Only if you sign in"</td>
                        <td>"To carry progress between your devices. Consent,
                            Art. 6(1)(a). Deleted on request, or automatically after two
                            years of not being used."</td>
                    </tr>
                </tbody>
            </table>
            <p>
                "Hosting providers may briefly process IP addresses in technical logs to
                deliver content and for security. We do not: load-balancer access logging
                is switched off and no visitor IP address is written to our application
                logs."
            </p>

            <h2>"Signing in"</h2>
            <p>
                "Signing in is optional and exists only to carry your progress between
                devices. It uses Google, and the only thing asked of Google is the "
                <code>"openid"</code>" scope — deliberately not "<code>"email"</code>
                " and not "<code>"profile"</code>". "
                <strong>
                    "We therefore never receive your email address or your name, and
                    cannot contact or identify you."
                </strong>
                " Google returns a pseudonymous identifier, and even that is not stored
                as-is: what we keep is a keyed hash of it."
            </p>
            <p>
                "One cookie is set, holding a signed session. It carries no tracking
                identifier, is not readable by any other site, lasts 90 days and is
                removed when you sign out. It is strictly necessary for signing in, which
                is why there is no banner asking about it."
            </p>
            <p>
                "There is a "<strong>"Delete my data"</strong>" button beside the sign-out
                link that erases your stored profile immediately, without involving us."
            </p>
            <p>
                "Because we hold no email address, "
                <strong>"there is no account recovery"</strong>
                ". If you lose access to your Google account, the progress behind it
                cannot be reached by you or by us. That is the trade for holding so
                little. Google's own record that you signed in is Google's processing,
                under its own privacy policy."
            </p>

            <h2>"Your rights"</h2>
            <p>
                "Under the GDPR you may request access, rectification, erasure,
                restriction, portability, and object to processing — email "
                <a href={format!("mailto:{PRIVACY_EMAIL}")}>{PRIVACY_EMAIL}</a>
                ". You may also lodge a complaint with your supervisory authority (in
                Belgium, the Gegevensbeschermingsautoriteit / APD)."
            </p>
            <p>
                "In practice there is little to ask about, because little is held. If you
                are signed in, the "<strong>"Delete my data"</strong>" button does the
                whole of erasure without a letter. A published leaderboard entry is
                identified only by a nickname you invented, so please quote it and roughly
                when you set it — which also means we cannot verify that a nickname is
                yours. We will honour the request anyway; that is the trade-off of keeping
                no identity."
            </p>

            <h2>"Changes"</h2>
            <p>
                "This site is open source, so changes to what it does are visible in its
                history. If it starts collecting something it does not collect today, this
                page changes in the same commit. The date above reflects the latest
                version."
            </p>

            <div class="contact">
                <p>
                    "Questions about any of this: "
                    <a href={format!("mailto:{PRIVACY_EMAIL}")}>{PRIVACY_EMAIL}</a>
                    " · "<a href="/terms">"Terms of Use"</a>
                    " · "<a href={IMPRINT_URL} rel="noreferrer">"Legal notice"</a>
                </p>
            </div>
        </section>
    }
}

/// The terms of use.
#[component]
pub fn Terms() -> impl IntoView {
    view! {
        <section class="legal">
            <header class="legal-header">
                <h1>"Terms of Use"</h1>
                <p class="legal-meta">{LAST_UPDATED}</p>
            </header>

            <p>
                "These Terms govern your use of Tinkhaven Typing (the \u{201c}Site\u{201d}),
                provided by "{CONTROLLER}" (\u{201c}Tinkhaven\u{201d}, \u{201c}we\u{201d}).
                By using the Site you agree to these Terms."
            </p>

            <h2>"The service"</h2>
            <p>
                "The Site is a free touch typing tutor. There is nothing to buy, no
                subscription and no account required. We may change or withdraw it at any
                time, and we do not promise it will be available without interruption."
            </p>

            <h2>"Licence"</h2>
            <p>
                "Tinkhaven Typing is a web port of "
                <a href="https://klavaro.sourceforge.io/" rel="noreferrer">"Klavaro"</a>
                " by Felipe Emmanuel Ferreira de Castro, and is free software under the "
                "GNU General Public Licence, version 3 or later. You may use, study, share
                and modify it on those terms. The source is at "
                <a href="https://github.com/tinkhaven/typing" rel="noreferrer">
                    "github.com/tinkhaven/typing"
                </a>", which is also the corresponding source for the WebAssembly this
                page runs."
            </p>
            <p>
                "The licence covers the software. It grants no rights to the Tinkhaven
                name or branding."
            </p>

            <h2>"Acceptable use"</h2>
            <p>
                "Use the Site lawfully and do not misuse our infrastructure — for example
                by attempting to disrupt or overload it. On the leaderboard, please choose
                a nickname that is not offensive and is not somebody else\u{2019}s name or
                personal data; entries that are may be removed without notice. The server
                scores runs itself and rejects impossible ones, but it is not proof
                against someone determined to fake a result, and it does not try to be."
            </p>

            <h2>"Intellectual property"</h2>
            <p>
                "The software is licensed as above. The Tinkhaven name, logo and branding
                are ours and these Terms grant no rights to them. Klavaro\u{2019}s data
                files and lessons remain the work of their author, reused under the GPL."
            </p>

            <h2>"Disclaimer and liability"</h2>
            <p>
                "The Site is provided \u{201c}as is\u{201d} and without warranty of any
                kind, as the GNU General Public Licence sets out. Speed, accuracy and
                fluidness figures are measurements of practice, not a certification of
                anything. Scores, progress and leaderboard entries may be lost. To the
                extent permitted by law, Tinkhaven is not liable for indirect or
                consequential damages. Nothing here limits liability that cannot be
                excluded under Belgian or EU law, including your statutory consumer
                rights."
            </p>

            <h2>"Privacy"</h2>
            <p>
                "Our "<a href="/privacy">"Privacy Policy"</a>
                " explains how we handle data — in short, we hold almost none."
            </p>

            <h2>"Changes"</h2>
            <p>
                "We may update these Terms; the date above reflects the latest version.
                Continued use means acceptance."
            </p>

            <h2>"Governing law"</h2>
            <p>
                "These Terms are governed by Belgian law, and the courts of Belgium have
                jurisdiction, without affecting any mandatory consumer protections
                available to you where you live."
            </p>

            <div class="contact">
                <p>
                    "Contact: "
                    <a href={format!("mailto:{PRIVACY_EMAIL}")}>{PRIVACY_EMAIL}</a>
                    " · "<a href="/privacy">"Privacy Policy"</a>
                    " · "<a href={IMPRINT_URL} rel="noreferrer">"Legal notice"</a>
                </p>
            </div>
        </section>
    }
}
