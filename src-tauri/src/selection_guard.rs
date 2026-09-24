//! Selection identity stays on one COM worker. Only an opaque lease crosses threads.
//!
//! A stalled accessibility provider must never create an unbounded queue of workers,
//! block the UI thread, or turn a timeout into permission to paste.

use crate::foreground::TargetSnapshot;

pub const MAX_TEXT_UNITS: usize = 65_536;

#[cfg(windows)]
mod windows;

pub struct SelectionGuard {
    #[cfg(windows)]
    id: u64,
}

pub enum BeginResult {
    #[cfg(windows)]
    Automatic(SelectionGuard),
    #[cfg(windows)]
    Manual(SelectionGuard),
    Denied,
}

impl SelectionGuard {
    pub fn begin(target: Option<TargetSnapshot>) -> BeginResult {
        #[cfg(windows)]
        {
            let id = windows::next_id();
            match windows::request(windows::Action::Begin { id, target }) {
                windows::Reply::Automatic => BeginResult::Automatic(Self { id }),
                windows::Reply::Manual => BeginResult::Manual(Self { id }),
                _ => BeginResult::Denied,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            BeginResult::Denied
        }
    }

    pub fn seal(&self, text: &str, via_select_all: bool) -> bool {
        #[cfg(windows)]
        {
            if text.encode_utf16().count() > MAX_TEXT_UNITS {
                return false;
            }
            windows::request(windows::Action::Seal {
                id: self.id,
                text: text.to_owned(),
                via_select_all,
            }) == windows::Reply::Approved
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
            windows::request(windows::Action::Check { id: self.id }) == windows::Reply::Approved
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn manual_matches(&self) -> bool {
        #[cfg(windows)]
        {
            windows::request(windows::Action::CheckManual { id: self.id })
                == windows::Reply::Approved
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
