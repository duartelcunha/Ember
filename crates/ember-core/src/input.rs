//! Pure keyboard ownership decisions shared by native adapters.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Accept,
    Reject,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyVerdict {
    PassThrough,
    Decide { decision: Decision, consume: bool },
}

/// Modifier presses alone do not express a preview decision.
pub fn is_modifier(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 // Generic Shift / Ctrl / Alt
            | 0x14 // Caps Lock
            | 0x5B | 0x5C // Left/right Windows key
            | 0xA0..=0xA5 // Left/right Shift / Ctrl / Alt
    )
}

/// Consume only confirmation keys. Ordinary typing rejects without consuming input.
pub fn classify_key(vk: u32) -> KeyVerdict {
    match vk {
        0x0D => KeyVerdict::Decide {
            decision: Decision::Accept,
            consume: true,
        }, // VK_RETURN
        0x1B => KeyVerdict::Decide {
            decision: Decision::Reject,
            consume: true,
        }, // VK_ESCAPE
        vk if is_modifier(vk) => KeyVerdict::PassThrough,
        _ => KeyVerdict::Decide {
            decision: Decision::Reject,
            consume: false,
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WatchVerdict {
    Cancel,
    ConsumeTail,
    ReleaseHeld,
    Pass,
}

/// An inherited Escape belongs to the target; a fresh Escape and its tail belong to Ember.
pub fn classify_watch_event(
    vk: u32,
    is_down: bool,
    ignoring_held: bool,
    decided: bool,
) -> WatchVerdict {
    if vk != 0x1B {
        return WatchVerdict::Pass;
    }
    if decided {
        return WatchVerdict::ConsumeTail;
    }
    if is_down {
        if ignoring_held {
            return WatchVerdict::Pass;
        }
        return WatchVerdict::Cancel;
    }
    if ignoring_held {
        return WatchVerdict::ReleaseHeld;
    }
    WatchVerdict::Pass
}

/// Silence never authorizes application.
pub const PREVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerOutcome {
    Committed(usize),
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_and_esc_answer_the_question_and_never_reach_the_app() {
        assert_eq!(
            classify_key(0x0D),
            KeyVerdict::Decide {
                decision: Decision::Accept,
                consume: true
            }
        );
        assert_eq!(
            classify_key(0x1B),
            KeyVerdict::Decide {
                decision: Decision::Reject,
                consume: true
            }
        );
    }

    #[test]
    fn typing_on_means_keep_the_original_without_eating_the_keystroke() {
        for vk in [
            0x41, /* A */
            0x20, /* Space */
            0x08, /* Backspace */
            0x09, /* Tab */
        ] {
            assert_eq!(
                classify_key(vk),
                KeyVerdict::Decide {
                    decision: Decision::Reject,
                    consume: false
                },
                "vk {vk:#x} devia manter o original sem consumir a tecla"
            );
        }
    }

    #[test]
    fn watch_a_fresh_esc_cancels_and_is_consumed_with_its_tail() {
        assert_eq!(
            classify_watch_event(0x1B, true, false, false),
            WatchVerdict::Cancel
        );
        assert_eq!(
            classify_watch_event(0x1B, true, false, true),
            WatchVerdict::ConsumeTail
        );
        assert_eq!(
            classify_watch_event(0x1B, false, false, true),
            WatchVerdict::ConsumeTail
        );
    }

    #[test]
    fn watch_an_esc_held_from_before_belongs_to_the_users_app() {
        assert_eq!(
            classify_watch_event(0x1B, true, true, false),
            WatchVerdict::Pass
        );
        assert_eq!(
            classify_watch_event(0x1B, false, true, false),
            WatchVerdict::ReleaseHeld
        );
    }

    #[test]
    fn watch_ignores_every_other_key_entirely() {
        for vk in [0x0D, 0x41, 0x20, 0x25, 0x26, 0x10, 0x11] {
            assert_eq!(
                classify_watch_event(vk, true, false, false),
                WatchVerdict::Pass,
                "vk {vk:#x} nao e assunto do watcher"
            );
        }
    }

    #[test]
    fn modifiers_alone_do_not_dismiss_the_preview() {
        for vk in [0x10, 0x11, 0x12, 0x14, 0x5B, 0xA0, 0xA2, 0xA5] {
            assert_eq!(
                classify_key(vk),
                KeyVerdict::PassThrough,
                "vk {vk:#x} e um modificador e nao devia decidir nada"
            );
        }
    }
}
