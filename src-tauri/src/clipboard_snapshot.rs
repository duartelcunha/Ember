//! Bounded Windows clipboard snapshots. Non-memory formats fail before mutation.
use windows::Win32::{
    Foundation::{GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL, HWND},
    System::{DataExchange::*, Memory::*},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    },
};

pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}
struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
struct Owner(HWND);
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
struct Allocation(HGLOBAL);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            let _ = GlobalFree(Some(self.0));
        }
    }
}

impl Snapshot {
    pub fn read() -> Result<Self, String> {
        unsafe { OpenClipboard(None) }.map_err(|_| "Clipboard is busy")?;
        let _open = Open;
        let mut ids = Vec::new();
        unsafe { SetLastError(ERROR_SUCCESS) };
        let mut id = unsafe { EnumClipboardFormats(0) };
        while id != 0 {
            if ids.len() >= 64 {
                return Err("Clipboard has too many formats to preserve".into());
            }
            ids.push(id);
            unsafe { SetLastError(ERROR_SUCCESS) };
            id = unsafe { EnumClipboardFormats(id) };
        }
        if unsafe { GetLastError() } != ERROR_SUCCESS {
            return Err("Clipboard formats could not be enumerated".into());
        }
        let has_dib = ids.contains(&8) || ids.contains(&17);
        let mut formats = Vec::new();
        let mut total = 0usize;
        for id in ids {
            // Windows synthesizes CF_BITMAP from a preserved DIB. Never treat an HBITMAP as HGLOBAL.
            if id == 2 && has_dib {
                continue;
            }
            if !matches!(id, 1 | 7 | 8 | 13 | 15 | 16 | 17 | 0xc000..=0xffff) {
                return Err("Clipboard contains a format that cannot be preserved safely".into());
            }
            let handle =
                unsafe { GetClipboardData(id) }.map_err(|_| "Clipboard format is unavailable")?;
            let global = HGLOBAL(handle.0);
            let size = unsafe { GlobalSize(global) };
            total = total.checked_add(size).ok_or("Clipboard size overflow")?;
            if size == 0 || total > 32 * 1024 * 1024 {
                return Err("Clipboard exceeds the preservation limit".into());
            }
            let pointer = unsafe { GlobalLock(global) };
            if pointer.is_null() {
                return Err("Clipboard format is not global memory".into());
            }
            let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
            unsafe {
                let _ = GlobalUnlock(global);
            }
            formats.push((id, bytes));
        }
        Ok(Self { formats })
    }

    /// Compare ownership while holding the clipboard lock. Another application's copy wins.
    pub fn restore_if_owned(&self, revision: u64) -> Result<bool, String> {
        // Allocate every format before emptying the clipboard. Allocation failure leaves it intact.
        let mut allocations = Vec::new();
        for (id, bytes) in &self.formats {
            let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
                .map_err(|_| "Clipboard allocation failed")?;
            let allocation = Allocation(global);
            let pointer = unsafe { GlobalLock(global) };
            if pointer.is_null() {
                return Err("Clipboard allocation could not be locked".into());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len());
                let _ = GlobalUnlock(global);
            }
            allocations.push((*id, allocation));
        }
        let owner = Owner(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    windows::core::w!("STATIC"),
                    windows::core::w!("Ember clipboard"),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            }
            .map_err(|_| "Clipboard owner could not be created")?,
        );
        unsafe { OpenClipboard(Some(owner.0)) }.map_err(|_| "Clipboard is busy")?;
        let _open = Open;
        if unsafe { GetClipboardSequenceNumber() } as u64 != revision {
            return Ok(false);
        }
        unsafe { EmptyClipboard() }.map_err(|_| "Clipboard could not be restored")?;
        for (id, allocation) in allocations {
            unsafe { SetClipboardData(id, Some(HANDLE(allocation.0 .0))) }
                .map_err(|_| "Clipboard format could not be restored")?;
            // Windows owns successful allocations until the next clipboard replacement.
            std::mem::forget(allocation);
        }
        Ok(true)
    }
}
