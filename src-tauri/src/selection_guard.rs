//! Selection identity stays on one COM worker. Only an opaque lease crosses threads.
//!
//! A stalled accessibility provider must never create an unbounded queue of workers,
//! block the UI thread, or turn a timeout into permission to paste.

use crate::foreground::TargetSnapshot;

#[cfg(windows)]
mod windows;

pub struct SelectionGuard {
    #[cfg(windows)]
    id: u64,
}

impl SelectionGuard {
    pub fn begin(target: Option<TargetSnapshot>) -> Option<Self> {
        #[cfg(windows)]
        {
            let id = windows::next_id();
            windows::request(windows::Action::Begin { id, target }).then_some(Self { id })
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            None
        }
    }

    pub fn seal(&self, text: &str, via_select_all: bool) -> bool {
        #[cfg(windows)]
        {
            if text.encode_utf16().count() > windows::MAX_TEXT_UNITS {
                return false;
            }
            windows::request(windows::Action::Seal {
                id: self.id,
                text: text.to_owned(),
                via_select_all,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (text, via_select_all);
            false
        }
    }

    pub fn matches(&self) -> bool {
        #[cfg(windows)]
        {
            windows::request(windows::Action::Check { id: self.id })
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

impl Drop for SelectionGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        windows::release(self.id);
    }
}
