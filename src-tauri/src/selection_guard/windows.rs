use super::{TargetSnapshot, MAX_TEXT_UNITS};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};
use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BOOL, VT_UNKNOWN};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextRange, IUIAutomationValuePattern, UIA_EditControlTypeId,
    UIA_IsReadOnlyAttributeId, UIA_TextPatternId, UIA_ValuePatternId,
};

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
    CheckManual {
        id: u64,
    },
    Release {
        id: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Denied,
    Automatic,
    Manual,
    Approved,
}

struct Request {
    action: Action,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    response: mpsc::SyncSender<Reply>,
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

pub fn request(action: Action) -> Reply {
    let Some(sender) = sender() else {
        return Reply::Denied;
    };
    exchange(sender, action, CALL_TIMEOUT)
}

fn exchange(sender: &mpsc::SyncSender<Request>, action: Action, timeout: Duration) -> Reply {
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
        return Reply::Denied;
    }
    let accepted = rx.recv_timeout(timeout).unwrap_or(Reply::Denied);
    cancelled.store(true, Ordering::Release);
    if Instant::now() < deadline {
        accepted
    } else {
        Reply::Denied
    }
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
    kind: AnchorKind,
    created: Instant,
}

enum AnchorKind {
    Automatic {
        range: IUIAutomationTextRange,
        initially_empty: bool,
        original_digest: [u8; 32],
        sealed: bool,
        full_field: bool,
    },
    Manual,
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
                let (next, reply) = target
                    .map(|target| capture(automation, id, target))
                    .unwrap_or((None, Reply::Denied));
                anchor = next;
                reply
            }
            Action::Seal {
                id,
                ref text,
                via_select_all,
            } => {
                if anchor
                    .as_mut()
                    .filter(|a| a.id == id)
                    .is_some_and(|a| seal(automation, a, text, via_select_all))
                {
                    Reply::Approved
                } else {
                    Reply::Denied
                }
            }
            Action::Check { id } => {
                if anchor
                    .as_ref()
                    .filter(|a| a.id == id)
                    .is_some_and(|a| check(automation, a))
                {
                    Reply::Approved
                } else {
                    Reply::Denied
                }
            }
            Action::CheckManual { id } => {
                if anchor
                    .as_ref()
                    .filter(|a| a.id == id)
                    .is_some_and(|a| check_manual(automation, a))
                {
                    Reply::Approved
                } else {
                    Reply::Denied
                }
            }
            Action::Release { id } => {
                if anchor.as_ref().is_some_and(|a| a.id == id) {
                    anchor = None
                }
                Reply::Approved
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

enum Focused {
    Automatic {
        element: IUIAutomationElement,
        range: IUIAutomationTextRange,
        pattern: IUIAutomationTextPattern,
    },
    Manual(IUIAutomationElement),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadOnlyState {
    Editable,
    ReadOnly,
    Unsupported,
    Unknown,
}

fn focused_candidate(automation: &IUIAutomation2, target: TargetSnapshot) -> Option<Focused> {
    if !crate::foreground::same_target(Some(target)) {
        return refused("target_changed");
    }
    unsafe {
        // GetFocusedElement is the focus source. Chromium can disagree in the extra focus
        // property, so an Edit with independent editability proof may proceed.
        let Ok(element) = automation.GetFocusedElement() else {
            return refused("no_focused_element");
        };
        if element.CurrentIsPassword().ok() != Some(false.into()) {
            return refused("password_or_unknown");
        }
        if element.CurrentIsEnabled().ok() != Some(true.into()) {
            return refused("element_disabled_or_unknown");
        }
        let is_edit = element.CurrentControlType().ok() == Some(UIA_EditControlTypeId);
        let has_focus = element
            .CurrentHasKeyboardFocus()
            .ok()
            .is_some_and(|value| value.as_bool());
        let value_editable = is_edit
            && element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|value| value.CurrentIsReadOnly().ok())
                .is_some_and(|read_only| !read_only.as_bool());
        if !has_focus && !value_editable {
            return refused("no_keyboard_focus");
        }
        let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        else {
            return manual_or_refused(
                element,
                target,
                is_edit && value_editable,
                "no_text_pattern",
            );
        };
        let Ok(ranges) = pattern.GetSelection() else {
            return manual_or_refused(element, target, is_edit && value_editable, "no_selection");
        };
        if ranges.Length().ok() != Some(1) {
            return manual_or_refused(
                element,
                target,
                is_edit && value_editable,
                "selection_not_single",
            );
        }
        let Ok(range) = ranges.GetElement(0) else {
            return refused("selection_unavailable");
        };
        let selection_state = read_only_state(automation, &range);
        let document_editable = (is_edit
            && (selection_state == ReadOnlyState::Unsupported || !has_focus))
            .then(|| pattern.DocumentRange().ok())
            .flatten()
            .is_some_and(|document| {
                read_only_state(automation, &document) == ReadOnlyState::Editable
            });
        let editable = authorizes_automatic(
            has_focus,
            is_edit,
            value_editable,
            selection_state,
            document_editable,
        );
        if !editable {
            return refused("read_only_or_unknown");
        }
        if !crate::foreground::same_target(Some(target)) {
            return refused("target_changed_late");
        }
        Some(Focused::Automatic {
            element,
            range,
            pattern,
        })
    }
}

fn manual_or_refused(
    element: IUIAutomationElement,
    target: TargetSnapshot,
    eligible: bool,
    why: &str,
) -> Option<Focused> {
    if eligible && crate::foreground::same_target(Some(target)) {
        log::info!("guard: manual {why}");
        Some(Focused::Manual(element))
    } else {
        refused(why)
    }
}

fn authorizes_automatic(
    has_focus: bool,
    is_edit: bool,
    value_editable: bool,
    selection: ReadOnlyState,
    document_editable: bool,
) -> bool {
    match selection {
        ReadOnlyState::Editable if has_focus => true,
        ReadOnlyState::Editable | ReadOnlyState::Unsupported => {
            is_edit && value_editable && document_editable
        }
        ReadOnlyState::ReadOnly | ReadOnlyState::Unknown => false,
    }
}

fn focused_selection(
    automation: &IUIAutomation2,
    target: TargetSnapshot,
) -> Option<(
    IUIAutomationElement,
    IUIAutomationTextRange,
    IUIAutomationTextPattern,
)> {
    match focused_candidate(automation, target)? {
        Focused::Automatic {
            element,
            range,
            pattern,
        } => Some((element, range, pattern)),
        Focused::Manual(_) => None,
    }
}

/// One word in the log, never the text. `None` typed so it drops into any `Option` return.
fn refused<T>(why: &str) -> Option<T> {
    log::info!("guard: refused {why}");
    None
}

fn read_only_state(automation: &IUIAutomation2, range: &IUIAutomationTextRange) -> ReadOnlyState {
    let Ok(mut value) = (unsafe { range.GetAttributeValue(UIA_IsReadOnlyAttributeId) }) else {
        return ReadOnlyState::Unknown;
    };
    let state = if is_editable_value(&value) {
        ReadOnlyState::Editable
    } else if unsafe { value.Anonymous.Anonymous.vt == VT_BOOL } {
        ReadOnlyState::ReadOnly
    } else if is_unsupported_value(automation, &value) {
        ReadOnlyState::Unsupported
    } else {
        ReadOnlyState::Unknown
    };
    // UIA can return a COM object for reserved values; release it on the worker apartment.
    let _ = unsafe { VariantClear(&mut value) };
    state
}

fn is_editable_value(value: &VARIANT) -> bool {
    // UIA's unsupported/mixed attributes are objects, never evidence of editability.
    (unsafe { value.Anonymous.Anonymous.vt == VT_BOOL }) && bool::try_from(value) == Ok(false)
}

fn is_unsupported_value(automation: &IUIAutomation2, value: &VARIANT) -> bool {
    unsafe {
        if value.Anonymous.Anonymous.vt != VT_UNKNOWN {
            return false;
        }
        let Ok(unsupported) = automation.ReservedNotSupportedValue() else {
            return false;
        };
        value
            .Anonymous
            .Anonymous
            .Anonymous
            .punkVal
            .as_ref()
            .is_some_and(|actual| actual.as_raw() == unsupported.as_raw())
    }
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

fn capture(
    automation: &IUIAutomation2,
    id: u64,
    target: TargetSnapshot,
) -> (Option<Anchor>, Reply) {
    let Some(focused) = focused_candidate(automation, target) else {
        return (None, Reply::Denied);
    };
    let (element, kind, reply) = match focused {
        Focused::Automatic { element, range, .. } => {
            let Some(original) = text(&range) else {
                return (None, Reply::Denied);
            };
            let Ok(range) = (unsafe { range.Clone() }) else {
                return (None, Reply::Denied);
            };
            (
                element,
                AnchorKind::Automatic {
                    range,
                    initially_empty: original.is_empty(),
                    original_digest: digest(&original),
                    sealed: false,
                    full_field: false,
                },
                Reply::Automatic,
            )
        }
        Focused::Manual(element) => (element, AnchorKind::Manual, Reply::Manual),
    };
    (
        Some(Anchor {
            id,
            target,
            element,
            kind,
            created: Instant::now(),
        }),
        reply,
    )
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
    let AnchorKind::Automatic {
        range: original_range,
        initially_empty,
        original_digest,
        sealed,
        ..
    } = &a.kind
    else {
        return false;
    };
    if *sealed {
        return false;
    }
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
        // Chromium may expose different endpoints for a Ctrl+A selection and its
        // document range even when both contain the entire same text. Require
        // the document text as a second full-field proof in that case.
        if !*initially_empty
            || read_only_state(automation, &document) != ReadOnlyState::Editable
            || (!same_range(&document, &range) && text(&document).as_deref() != Some(expected))
        {
            return false;
        }
    } else if *initially_empty
        || digest(expected) != *original_digest
        || !same_range(original_range, &range)
    {
        return false;
    }
    let Ok(cloned) = (unsafe { range.Clone() }) else {
        return false;
    };
    if let AnchorKind::Automatic {
        range,
        original_digest,
        sealed,
        full_field,
        ..
    } = &mut a.kind
    {
        *range = cloned;
        *original_digest = digest(expected);
        *sealed = true;
        *full_field = via_select_all;
    }
    true
}

fn check(automation: &IUIAutomation2, a: &Anchor) -> bool {
    let AnchorKind::Automatic {
        range: original_range,
        original_digest,
        sealed: true,
        full_field,
        ..
    } = &a.kind
    else {
        return false;
    };
    let Some((element, range, pattern)) = focused_selection(automation, a.target) else {
        return false;
    };
    same_element(automation, a, &element)
        && same_range(original_range, &range)
        && text(&range).is_some_and(|value| digest(&value) == *original_digest)
        && (!*full_field
            || unsafe { pattern.DocumentRange() }
                .ok()
                .is_some_and(|document| {
                    read_only_state(automation, &document) == ReadOnlyState::Editable
                        && text(&document).is_some_and(|value| digest(&value) == *original_digest)
                }))
}

fn check_manual(automation: &IUIAutomation2, a: &Anchor) -> bool {
    if !matches!(a.kind, AnchorKind::Manual) {
        return false;
    }
    let Some(candidate) = focused_candidate(automation, a.target) else {
        return false;
    };
    let element = match candidate {
        Focused::Automatic { element, .. } | Focused::Manual(element) => element,
    };
    same_element(automation, a, &element) && crate::foreground::same_target(Some(a.target))
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
    fn native_unsupported_sentinel_is_distinct_from_an_unknown_value() {
        std::thread::spawn(|| {
            let client = Client::new().expect("Windows UI Automation client must initialize");
            let sentinel = unsafe { client.automation.ReservedNotSupportedValue() }
                .expect("UI Automation must expose the unsupported sentinel");
            assert!(is_unsupported_value(
                &client.automation,
                &VARIANT::from(sentinel)
            ));
            assert!(!is_unsupported_value(
                &client.automation,
                &VARIANT::from(false)
            ));
        })
        .join()
        .unwrap();
    }

    #[test]
    #[ignore = "requires a focused Codex composer on the interactive desktop"]
    fn focused_codex_composer_is_an_automatic_candidate() {
        let exe = crate::foreground::debug_foreground_exe().unwrap_or_default();
        assert!(
            exe.to_ascii_lowercase().ends_with("chatgpt.exe"),
            "the Codex desktop application must own the foreground"
        );
        let target = crate::foreground::foreground_target().expect("focused native target");
        let client = Client::new().expect("Windows UI Automation client must initialize");
        assert!(matches!(
            focused_candidate(&client.automation, target),
            Some(Focused::Automatic { .. })
        ));
    }

    #[test]
    #[ignore = "requires the focused Codex composer to contain the qualification text"]
    fn focused_codex_composer_can_seal_a_full_field_selection() {
        use ember_core::selection::SelectionIo;

        const SAMPLE: &str = "Ember Codex full field qualification text.";
        let deadline = Instant::now() + Duration::from_secs(15);
        let exe = loop {
            let exe = crate::foreground::debug_foreground_exe().unwrap_or_default();
            if exe.to_ascii_lowercase().ends_with("chatgpt.exe") || Instant::now() >= deadline {
                break exe;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(exe.to_ascii_lowercase().ends_with("chatgpt.exe"));
        std::thread::sleep(Duration::from_millis(300));
        let target = crate::foreground::foreground_target().expect("focused native target");
        let guard = match crate::selection_guard::SelectionGuard::begin(Some(target)) {
            crate::selection_guard::BeginResult::Automatic(guard) => guard,
            _ => panic!("Codex composer must support automatic capture"),
        };
        let mut io = crate::selection::RealIo::new(false).expect("native input");
        io.send_select_all();
        assert!(io.input_succeeded(), "select-all input must succeed");
        assert!(guard.seal(SAMPLE, true), "full field selection must seal");
        assert!(guard.matches(), "the sealed field must still match");
    }

    #[test]
    fn unsupported_selection_attribute_needs_two_editability_proofs() {
        assert!(authorizes_automatic(
            false,
            true,
            true,
            ReadOnlyState::Unsupported,
            true
        ));
        assert!(!authorizes_automatic(
            false,
            true,
            false,
            ReadOnlyState::Unsupported,
            true
        ));
        assert!(!authorizes_automatic(
            false,
            true,
            true,
            ReadOnlyState::Unsupported,
            false
        ));
        assert!(!authorizes_automatic(
            false,
            false,
            true,
            ReadOnlyState::Unsupported,
            true
        ));
    }

    #[test]
    fn read_only_and_unknown_selection_never_authorize_replacement() {
        for selection in [ReadOnlyState::ReadOnly, ReadOnlyState::Unknown] {
            assert!(!authorizes_automatic(true, true, true, selection, true));
        }
        assert!(authorizes_automatic(
            true,
            false,
            false,
            ReadOnlyState::Editable,
            false
        ));
        assert!(!authorizes_automatic(
            false,
            true,
            true,
            ReadOnlyState::Editable,
            false
        ));
    }

    #[test]
    fn stalled_provider_times_out_and_invalidates_the_request() {
        let (tx, rx) = mpsc::sync_channel(1);
        assert_eq!(
            exchange(&tx, Action::Check { id: 1 }, Duration::from_millis(5)),
            Reply::Denied
        );
        let pending = rx.recv().unwrap();
        assert!(!request_live(&pending));
        assert!(pending.response.try_send(Reply::Approved).is_err());
    }

    #[test]
    fn full_provider_mailbox_fails_closed_without_queueing_more_work() {
        let (tx, rx) = mpsc::sync_channel(1);
        assert_eq!(
            exchange(&tx, Action::Check { id: 1 }, Duration::from_millis(1)),
            Reply::Denied
        );
        assert_eq!(
            exchange(&tx, Action::Check { id: 2 }, Duration::from_secs(1)),
            Reply::Denied
        );
        assert!(matches!(rx.recv().unwrap().action, Action::Check { id: 1 }));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn disconnected_provider_never_authorizes_application() {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(rx);
        assert_eq!(
            exchange(&tx, Action::Check { id: 1 }, Duration::from_secs(1)),
            Reply::Denied
        );
    }
}
