//! The on-screen keyboard.
//!
//! Klavaro's whole approach rests on never looking at your hands, so the picture
//! on screen has to carry the information the keyboard would: which key comes
//! next, and which finger belongs on it. Keys are coloured by finger — the same
//! nine colours Klavaro uses — and the next key is highlighted, with its Shift
//! partner lit up when the character needs one.
//!
//! Drawn as a grid of `<div>`s rather than an SVG: it is a grid, the browser
//! already lays those out, and text in a div stays selectable and legible at any
//! zoom.

use std::sync::LazyLock;

use leptos::prelude::*;
use typing_core::kbd::{Finger, FingerMap, Hand, KeyPos, Layout, COLS, ROWS};

/// The finger map is one file shared by every layout, so parse it once.
static FINGERS: LazyLock<FingerMap> = LazyLock::new(FingerMap::klavaro);

/// Where a row's keys sit relative to the row above, in key widths.
///
/// Real keyboards stagger their rows. Without this the columns line up in a way
/// that makes the finger colours look wrong.
const ROW_OFFSETS: [f32; ROWS] = [0.0, 0.5, 0.75, 0.25];

/// Draws the keyboard, highlighting the key to press next.
#[component]
pub fn VirtualKeyboard(
    /// The layout to draw.
    #[prop(into)]
    layout: Signal<Layout>,
    /// The character the typist should produce next, if any.
    #[prop(into)]
    next: Signal<Option<char>>,
) -> impl IntoView {
    // Where the next character lives on this layout, and whether Shift is needed.
    let target = Memo::new(move |_| next.get().and_then(|ch| layout.read().find(ch)));
    let needs_shift = Memo::new(move |_| target.get().is_some_and(|pos| pos.shifted));
    let shift_hand = Memo::new(move |_| {
        // Shift is pressed with the hand *opposite* the one reaching the key.
        target.get().and_then(|pos| {
            FINGERS.at(pos.row, pos.col).map(|finger| match finger.hand() {
                Hand::Left => Hand::Right,
                Hand::Right => Hand::Left,
                Hand::Either => Hand::Either,
            })
        })
    });

    view! {
        <div class="keyboard" aria-hidden="true">
            {(0..ROWS)
                .map(|row| {
                    view! {
                        <div
                            class="keyboard-row"
                            style=format!("padding-left: {}em", ROW_OFFSETS[row] * 2.6)
                        >
                            {if row == ROWS - 1 {
                                Some(
                                    view! {
                                        <ShiftKey
                                            hand=Hand::Left
                                            lit=Signal::derive(move || {
                                                needs_shift.get()
                                                    && matches!(
                                                        shift_hand.get(),
                                                        Some(Hand::Left) | Some(Hand::Either)
                                                    )
                                            })
                                        />
                                    },
                                )
                            } else {
                                None
                            }}
                            {(0..COLS)
                                .map(|col| {
                                    view! { <Key layout=layout target=target row=row col=col /> }
                                })
                                .collect_view()}
                            {if row == ROWS - 1 {
                                Some(
                                    view! {
                                        <ShiftKey
                                            hand=Hand::Right
                                            lit=Signal::derive(move || {
                                                needs_shift.get()
                                                    && matches!(
                                                        shift_hand.get(),
                                                        Some(Hand::Right) | Some(Hand::Either)
                                                    )
                                            })
                                        />
                                    },
                                )
                            } else {
                                None
                            }}
                        </div>
                    }
                })
                .collect_view()}
            <div class="keyboard-row keyboard-row-space">
                <div
                    class=move || {
                        let lit = next.get() == Some(' ');
                        format!("key key-space finger-{}{}", Finger::Thumb.slot(), if lit { " key-next" } else { "" })
                    }
                >
                    <span class="key-label">"space"</span>
                </div>
            </div>
        </div>
    }
}

/// One key of the layout.
#[component]
fn Key(
    #[prop(into)] layout: Signal<Layout>,
    #[prop(into)] target: Memo<Option<KeyPos>>,
    row: usize,
    col: usize,
) -> impl IntoView {
    let lower = Memo::new(move |_| layout.read().lower(row, col));
    let upper = Memo::new(move |_| layout.read().upper(row, col));
    let finger = FINGERS.at(row, col);

    let class = move || {
        let mut class = String::from("key");
        if lower.get().is_none() && upper.get().is_none() {
            class.push_str(" key-blank");
            return class;
        }
        if let Some(finger) = finger {
            class.push_str(&format!(" finger-{}", finger.slot()));
        }
        if target.get().is_some_and(|pos| pos.row == row && pos.col == col) {
            class.push_str(" key-next");
        }
        class
    };

    view! {
        <div class=class>
            {move || {
                upper
                    .get()
                    .filter(|u| Some(*u) != lower.get())
                    .map(|u| view! { <span class="key-shifted">{u.to_string()}</span> })
            }}
            <span class="key-label">
                {move || lower.get().or_else(|| upper.get()).map(|c| c.to_string()).unwrap_or_default()}
            </span>
        </div>
    }
}

/// A Shift key. Not part of the layout grid, but part of what to press.
#[component]
fn ShiftKey(hand: Hand, #[prop(into)] lit: Signal<bool>) -> impl IntoView {
    let finger = match hand {
        Hand::Left => Finger::LeftLittle,
        // Either should not reach here; little finger is the safe answer.
        Hand::Right | Hand::Either => Finger::RightLittle,
    };
    let class = move || {
        format!(
            "key key-shift finger-{}{}",
            finger.slot(),
            if lit.get() { " key-next" } else { "" }
        )
    };
    view! {
        <div class=class>
            <span class="key-label">"shift"</span>
        </div>
    }
}
