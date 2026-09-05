use super::TargetSnapshot;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextRange, UIA_IsReadOnlyAttributeId, UIA_TextPatternId,
};

pub const MAX_TEXT_UNITS: usize = 65_536;
const CALL_TIMEOUT: Duration = Duration::from_millis(1500);
const LEASE_LIFETIME: Duration = Duration::from_secs(600);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static WORKER: OnceLock<Option<mpsc::SyncSender<Request>>> = OnceLock::new();

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub enum Action {
    Begin {
        id: u64,
        target: Option<TargetSnapshot>,
    },
    Seal {
        id: u64,
        text: String,
        via_select_all: bool,
    },
    Check {
        id: u64,
    },
    Release {
        id: u64,
    },
}

struct Request {
    action: Action,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    response: mpsc::SyncSender<bool>,
}

fn sender() -> Option<&'static mpsc::SyncSender<Request>> {
    WORKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("ember-selection-accessibility".into())
                .spawn(move || worker(rx))
                .ok()?;
            Some(tx)
        })
        .as_ref()
}

pub fn request(action: Action) -> bool {
    let Some(sender) = sender() else { return false };
    exchange(sender, action, CALL_TIMEOUT)
}

fn exchange(sender: &mpsc::SyncSender<Request>, action: Action, timeout: Duration) -> bool {
    let (tx, rx) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    let deadline = Instant::now() + timeout;
    if sender
        .try_send(Request {
            action,
            deadline,
            cancelled: cancelled.clone(),
            response: tx,
        })
        .is_err()
    {
        return false;
    }
    let accepted = rx.recv_timeout(timeout).unwrap_or(false) && Instant::now() < deadline;
    cancelled.store(true, Ordering::Release);
    accepted
}

pub fn release(id: u64) {
    let Some(Some(sender)) = WORKER.get() else {
        return;
    };
    let (response, _) = mpsc::sync_channel(1);
    // Drop cannot wait on a provider. A full mailbox is bounded and the lease also expires.
    let _ = sender.try_send(Request {
        action: Action::Release { id },
        deadline: Instant::now() + CALL_TIMEOUT,
        cancelled: Arc::new(AtomicBool::new(false)),
        response,
    });
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

struct Anchor {
    id: u64,
    target: TargetSnapshot,
    element: IUIAutomationElement,
    range: IUIAutomationTextRange,
    initially_empty: bool,
    original_digest: [u8; 32],
    sealed: bool,
    created: Instant,
}

struct Client {
    // Field order releases the interfaces before uninitializing their COM apartment.
    automation: IUIAutomation2,
    _apartment: Apartment,
}

impl Client {
    fn new() -> Option<Self> {
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            return None;
        }
        let apartment = Apartment;
        let automation: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }.ok()?;
        unsafe {
            automation
                .SetAutoSetFocus(false)
                .and_then(|()| automation.SetConnectionTimeout(250))
                .and_then(|()| automation.SetTransactionTimeout(250))
        }
        .ok()?;
        Some(Self {
            automation,
            _apartment: apartment,
        })
    }
}

fn worker(rx: mpsc::Receiver<Request>) {
    // All UIA objects are created, used and released on this MTA thread, never a UI/hook thread.
    let Some(client) = Client::new() else { return };
    let automation = &client.automation;
    let mut anchor: Option<Anchor> = None;
    loop {
        let request = match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(request) => request,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if anchor
                    .as_ref()
                    .is_some_and(|a| a.created.elapsed() >= LEASE_LIFETIME)
                {
                    anchor = None;
                }
                continue;
            }
        };
        if !request_live(&request) {
            continue;
        }
        let result = match request.action {
            Action::Begin { id, target } => {
                anchor = target.and_then(|target| capture(automation, id, target));
                anchor.is_some()
            }
            Action::Seal {
                id,
                ref text,
                via_select_all,
            } => anchor
                .as_mut()
                .filter(|a| a.id == id && !a.sealed)
                .is_some_and(|a| seal(automation, a, text, via_select_all)),
            Action::Check { id } => anchor
                .as_ref()
                .filter(|a| a.id == id && a.sealed)
                .is_some_and(|a| check(automation, a)),
            Action::Release { id } => {
                if anchor.as_ref().is_some_and(|a| a.id == id) {
                    anchor = None
                }
                true
            }
        };
        // An expired response cannot revive a lease even if the COM call eventually succeeds.
        if !request_live(&request) {
            anchor = None;
            continue;
        }
        let _ = request.response.try_send(result);
    }
}

fn request_live(request: &Request) -> bool {
    !request.cancelled.load(Ordering::Acquire) && Instant::now() < request.deadline
}

fn focused_selection(
    automation: &IUIAutomation2,
    target: TargetSnapshot,
) -> Option<(
    IUIAutomationElement,
    IUIAutomationTextRange,
    IUIAutomationTextPattern,
)> {
    if !crate::foreground::same_target(Some(target)) {
        return None;
    }
    unsafe {
        let element = automation.GetFocusedElement().ok()?;
        if element.CurrentIsPassword().ok()?.as_bool()
            || !element.CurrentIsEnabled().ok()?.as_bool()
            || !element.CurrentHasKeyboardFocus().ok()?.as_bool()
        {
            return None;
        }
        let pattern: IUIAutomationTextPattern =
            element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = pattern.GetSelection().ok()?;
        if ranges.Length().ok()? != 1 {
            return None;
        }
        let range = ranges.GetElement(0).ok()?;
        if !editable(&range) || !crate::foreground::same_target(Some(target)) {
            return None;
        }
        Some((element, range, pattern))
    }
}

fn editable(range: &IUIAutomationTextRange) -> bool {
    unsafe {
        let Ok(value) = range.GetAttributeValue(UIA_IsReadOnlyAttributeId) else {
            return false;
        };
        is_editable_value(&value)
    }
}

fn is_editable_value(value: &VARIANT) -> bool {
    // UIA's unsupported/mixed attributes are objects, never evidence of editability.
    (unsafe { value.Anonymous.Anonymous.vt == VT_BOOL }) && bool::try_from(value) == Ok(false)
}

fn text(range: &IUIAutomationTextRange) -> Option<String> {
    let value = unsafe { range.GetText((MAX_TEXT_UNITS + 1) as i32) }.ok()?;
    if value.len() > MAX_TEXT_UNITS {
        return None;
    }
    String::from_utf16(&value).ok()
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn capture(automation: &IUIAutomation2, id: u64, target: TargetSnapshot) -> Option<Anchor> {
    let (element, range, _) = focused_selection(automation, target)?;
    let original = text(&range)?;
    Some(Anchor {
        id,
        target,
        element,
        range: unsafe { range.Clone() }.ok()?,
        initially_empty: original.is_empty(),
        original_digest: digest(&original),
        sealed: false,
        created: Instant::now(),
    })
}

fn same_element(automation: &IUIAutomation2, a: &Anchor, element: &IUIAutomationElement) -> bool {
    a.created.elapsed() < LEASE_LIFETIME
        && unsafe { automation.CompareElements(&a.element, element) }.is_ok_and(|v| v.as_bool())
}

fn same_range(a: &IUIAutomationTextRange, b: &IUIAutomationTextRange) -> bool {
    // Compare checks both endpoints. Equal text at another position must not pass.
    unsafe { a.Compare(b) }.is_ok_and(|v| v.as_bool())
}

fn seal(automation: &IUIAutomation2, a: &mut Anchor, expected: &str, via_select_all: bool) -> bool {
    let Some((element, range, pattern)) = focused_selection(automation, a.target) else {
        return false;
    };
    if !same_element(automation, a, &element) || text(&range).as_deref() != Some(expected) {
        return false;
    }
    if via_select_all {
        let Ok(document) = (unsafe { pattern.DocumentRange() }) else {
            return false;
        };
        if !a.initially_empty || !editable(&document) || !same_range(&document, &range) {
            return false;
        }
    } else if a.initially_empty
        || digest(expected) != a.original_digest
        || !same_range(&a.range, &range)
    {
        return false;
    }
    let Ok(cloned) = (unsafe { range.Clone() }) else {
        return false;
    };
    a.range = cloned;
    a.original_digest = digest(expected);
    a.sealed = true;
    true
}

fn check(automation: &IUIAutomation2, a: &Anchor) -> bool {
    let Some((element, range, _)) = focused_selection(automation, a.target) else {
        return false;
    };
    same_element(automation, a, &element)
        && same_range(&a.range, &range)
        && text(&range).is_some_and(|value| digest(&value) == a.original_digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_client_uses_bounded_calls_without_setting_focus() {
        std::thread::spawn(|| {
            let client = Client::new().expect("Windows UI Automation client must initialize");
            unsafe {
                assert!(!client.automation.AutoSetFocus().unwrap().as_bool());
                assert_eq!(client.automation.ConnectionTimeout().unwrap(), 250);
                assert_eq!(client.automation.TransactionTimeout().unwrap(), 250);
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn unknown_or_coercible_readonly_attributes_are_not_editability_evidence() {
        assert!(is_editable_value(&VARIANT::from(false)));
        assert!(!is_editable_value(&VARIANT::from(true)));
        assert!(!is_editable_value(&VARIANT::default()));
        assert!(!is_editable_value(&VARIANT::from(0_i32)));
        assert!(!is_editable_value(&VARIANT::from("false")));
    }

    #[test]
    fn stalled_provider_times_out_and_invalidates_the_request() {
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(!exchange(
            &tx,
            Action::Check { id: 1 },
            Duration::from_millis(5)
        ));
        let pending = rx.recv().unwrap();
        assert!(!request_live(&pending));
        assert!(pending.response.try_send(true).is_err());
    }

    #[test]
    fn full_provider_mailbox_fails_closed_without_queueing_more_work() {
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(!exchange(
            &tx,
            Action::Check { id: 1 },
            Duration::from_millis(1)
        ));
        assert!(!exchange(
            &tx,
            Action::Check { id: 2 },
            Duration::from_secs(1)
        ));
        assert!(matches!(rx.recv().unwrap().action, Action::Check { id: 1 }));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn disconnected_provider_never_authorizes_application() {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(rx);
        assert!(!exchange(
            &tx,
            Action::Check { id: 1 },
            Duration::from_secs(1)
        ));
    }
}
