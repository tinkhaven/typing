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

/// How far each row is indented, in key widths.
///
/// Keyboard rows are staggered, and the stagger is what makes the finger
/// colours read as columns your hands actually move along. The bottom row has no
/// indent of its own: the Shift key stands in for its empty first column, which
/// is exactly what column zero is doing in the `.kbd` files.
const ROW_INDENTS: [f32; ROWS] = [0.0, 0.5, 0.75, 0.0];

/// One key width plus the gap after it, in `em`. Indents are multiples of this.
const KEY_PITCH_EM: f32 = 2.65;

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
            FINGERS
                .at(pos.row, pos.col)
                .map(|finger| match finger.hand() {
                    Hand::Left => Hand::Right,
                    Hand::Right => Hand::Left,
                    Hand::Either => Hand::Either,
                })
        })
    });

    // Which columns each row actually uses.
    //
    // Layout files are a fixed 14 columns wide and most rows do not fill them.
    // Rendering the empty ones put a stretch of nothing between `/` and the
    // right Shift, and left the bottom row's blank first column sitting between
    // Shift and `z`. So each row is drawn only across the span it occupies.
    let spans = Memo::new(move |_| {
        let layout = layout.read();
        let mut spans = [(0usize, 0usize); ROWS];
        for (row, span) in spans.iter_mut().enumerate() {
            let occupied: Vec<usize> = (0..COLS)
                .filter(|&col| layout.lower(row, col).is_some() || layout.upper(row, col).is_some())
                .collect();
            *span = match (occupied.first(), occupied.last()) {
                (Some(&first), Some(&last)) => (first, last),
                // A row with no keys at all: render nothing rather than 14 blanks.
                _ => (1, 0),
            };
        }
        spans
    });

    let lit_shift = move |hand: Hand| {
        Signal::derive(move || {
            needs_shift.get()
                && matches!(shift_hand.get(), Some(h) if h == hand || h == Hand::Either)
        })
    };

    view! {
        <div class="keyboard" aria-hidden="true">
            {(0..ROWS)
                .map(|row| {
                    let is_bottom = row == ROWS - 1;
                    view! {
                        <div
                            class="keyboard-row"
                            style=format!("padding-left: {}em", ROW_INDENTS[row] * KEY_PITCH_EM)
                        >
                            {is_bottom
                                .then(|| {
                                    view! { <ShiftKey hand=Hand::Left lit=lit_shift(Hand::Left) /> }
                                })}
                            {move || {
                                let (first, last) = spans.get()[row];
                                (first..=last)
                                    .map(|col| {
                                        view! { <Key layout=layout target=target row=row col=col /> }
                                    })
                                    .collect_view()
                            }}
                            {is_bottom
                                .then(|| {
                                    view! {
                                        <ShiftKey hand=Hand::Right lit=lit_shift(Hand::Right) />
                                    }
                                })}
                        </div>
                    }
                })
                .collect_view()}
            <div class="keyboard-row keyboard-row-space">
                <div class=move || {
                    let lit = next.get() == Some(' ');
                    format!(
                        "key key-space finger-{}{}",
                        Finger::Thumb.slot(),
                        if lit { " key-next" } else { "" },
                    )
                }>
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
            // A gap inside a row, e.g. a layout that skips a position.
            class.push_str(" key-blank");
            return class;
        }
        if let Some(finger) = finger {
            class.push_str(&format!(" finger-{}", finger.slot()));
        }
        if target
            .get()
            .is_some_and(|pos| pos.row == row && pos.col == col)
        {
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
                {move || {
                    lower.get().or_else(|| upper.get()).map(|c| c.to_string()).unwrap_or_default()
                }}
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
            if lit.get() { " key-next" } else { "" },
        )
    };
    view! {
        <div class=class>
            <span class="key-label">"shift"</span>
        </div>
    }
}
