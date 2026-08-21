//! Terminal input.
//!
//! Two jobs, both of which the render loop used to do badly by hand:
//!
//! 1. **Read continuously.** Reading only once per rendered frame means a
//!    terminal control sequence that a PTY read split after the initial `ESC`
//!    (crossterm-rs/crossterm#993) has its tail sitting in the buffer for a
//!    whole frame. By the time anyone looks, the pieces no longer look like one
//!    sequence, and a focus-in report (`ESC [ I`) turns into `Esc`, `[`, `I` —
//!    three bogus shortcuts. A dedicated thread that blocks on `event::read()`
//!    picks the tail up microseconds later, so reassembly is reliable and its
//!    timing window can stay short enough to never delay a real `Esc`.
//!
//! 2. **Hand over whole batches.** Auto-repeat delivers keys far faster than a
//!    full repaint. Processing one key per frame lets the queue grow without
//!    bound, so held keys keep scrolling long after release. [`InputReader`]
//!    exposes every event queued so far and lets the caller draw once.
//!
//! Reassembly never leaks an escape payload into shortcut handling: a partial
//! sequence is dropped rather than replayed byte by byte. It is also lossless
//! for real keys — anything that cannot appear inside the sequence being parsed
//! is handed back as the keypress it is.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

/// How long to look for something immediately behind a bare `Esc`.
///
/// A sequence is split by a read boundary, not by the terminal: the tail was
/// written in the same breath as the `ESC` and is already in the buffer, so the
/// reader thread sees it in microseconds. The budget only has to cover scheduler
/// jitter, so one frame is ample — and it is short enough that a real `Esc`
/// press feels immediate, which the old 25ms-per-byte-after-a-full-frame scheme
/// could not manage. No human types `Esc` then `[` inside this window.
const INTRODUCER_PROBE: Duration = Duration::from_millis(15);

/// How long to wait per byte once an introducer has confirmed a sequence is in
/// flight. At that point no real keypress is being held up, so this can be
/// generous enough to absorb a slow or descheduled terminal.
const CONTINUATION_WINDOW: Duration = Duration::from_millis(100);

/// Upper bound on how long one reassembly may take, so a terminal dribbling
/// bytes can never stall input.
const SEQUENCE_DEADLINE: Duration = Duration::from_millis(400);

/// Guard against a pathological run of parameter bytes.
const MAX_SEQUENCE_BYTES: usize = 64;

/// Most events handed to the UI in one batch. Keeps a paste storm or a very long
/// auto-repeat burst from starving the renderer.
const MAX_BATCH: usize = 512;

/// Reads terminal events on a dedicated thread and serves them in batches.
pub struct InputReader {
    rx: Receiver<Event>,
    /// When true the reader thread stops calling `event::poll`/`read` so another
    /// process (e.g. `hx`) can own stdin. Crossterm's reader is process-wide.
    paused: Arc<AtomicBool>,
}

impl InputReader {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let paused = Arc::new(AtomicBool::new(false));
        let paused_thread = Arc::clone(&paused);
        thread::Builder::new()
            .name("input-reader".into())
            .spawn(move || {
                // Exits when the receiver goes away at shutdown.
                loop {
                    if paused_thread.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    // Short polls so `pause()` can take effect without waiting
                    // on the old 1-hour `event::poll` (which steals hx's stdin).
                    match next_events_interruptible(&mut CrosstermSource, &paused_thread) {
                        Ok(None) => continue, // paused mid-wait or idle tick
                        Ok(Some(events)) => {
                            for event in events {
                                if tx.send(event).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(_) => return,
                    }
                }
            })
            .expect("spawn input reader thread");
        Self { rx, paused }
    }

    /// Stop polling the terminal so a subprocess can read stdin.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        // Wait for the reader to finish its current short poll (≤50ms).
        thread::sleep(Duration::from_millis(60));
    }

    /// Resume polling after a suspended subprocess exits.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Drop any events that arrived around a suspend/resume boundary.
    pub fn drain(&self) {
        while self.rx.try_recv().is_ok() {}
    }

    /// Wait up to `timeout` for input, then return it together with everything
    /// else already queued. Empty means the timeout expired with nothing to do.
    pub fn wait_batch(&self, timeout: Duration) -> Vec<Event> {
        let mut batch = Vec::new();
        match self.rx.recv_timeout(timeout) {
            Ok(event) => batch.push(event),
            Err(RecvTimeoutError::Timeout) => return batch,
            Err(RecvTimeoutError::Disconnected) => return batch,
        }
        while batch.len() < MAX_BATCH {
            match self.rx.try_recv() {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }
        batch
    }
}

/// Source of already-parsed crossterm events, abstracted so the reassembly state
/// machine can be tested without a terminal.
trait EventSource {
    /// Next event, or `None` if `timeout` expired first.
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>>;
}

struct CrosstermSource;

impl EventSource for CrosstermSource {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        if timeout.is_zero() {
            return Ok(None);
        }
        if event::poll(timeout)? {
            return event::read().map(Some);
        }
        Ok(None)
    }
}

const PAUSE_POLL: Duration = Duration::from_millis(50);

/// Like [`next_events`], but returns `Ok(None)` when `paused` becomes true so
/// the reader thread can yield the tty to a suspended editor.
fn next_events_interruptible(
    source: &mut impl EventSource,
    paused: &AtomicBool,
) -> io::Result<Option<Vec<Event>>> {
    let first = loop {
        if paused.load(Ordering::SeqCst) {
            return Ok(None);
        }
        if let Some(event) = source.next(PAUSE_POLL)? {
            break event;
        }
    };
    let Event::Key(key) = first else {
        return Ok(Some(vec![first]));
    };
    if key.code != KeyCode::Esc || key.kind != KeyEventKind::Press {
        return Ok(Some(vec![Event::Key(key)]));
    }
    resolve_escape(source).map(Some)
}

/// Blocking read of the next logical event(s).
fn next_events(source: &mut impl EventSource) -> io::Result<Vec<Event>> {
    // A bare `Esc` is the only event that can be the head of a sequence that
    // crossterm failed to keep together, so everything else passes straight
    // through.
    let first = loop {
        if let Some(event) = source.next(Duration::from_secs(3600))? {
            break event;
        }
    };
    let Event::Key(key) = first else {
        return Ok(vec![first]);
    };
    if key.code != KeyCode::Esc || key.kind != KeyEventKind::Press {
        return Ok(vec![Event::Key(key)]);
    }
    resolve_escape(source)
}

/// Decide what a bare `Esc` actually was: a keypress, an `Alt`-modified key, or
/// the start of a control sequence that arrived in pieces.
fn resolve_escape(source: &mut impl EventSource) -> io::Result<Vec<Event>> {
    let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    let Some(second) = source.next(INTRODUCER_PROBE)? else {
        return Ok(vec![esc]);
    };
    // Anything that is not a plain character cannot continue a sequence, so
    // deliver both rather than dropping either.
    let Event::Key(mut key) = second else {
        return Ok(vec![esc, second]);
    };
    let KeyCode::Char(introducer) = key.code else {
        return Ok(vec![esc, Event::Key(key)]);
    };

    let deadline = Instant::now() + SEQUENCE_DEADLINE;
    match introducer {
        '[' => read_csi(source, deadline),
        'O' => read_ss3(source, deadline),
        // OSC/DCS/PM/APC carry a string payload that is never a shortcut. Read
        // to its terminator and emit nothing.
        ']' | 'P' | '^' | '_' => {
            consume_control_string(source, deadline)?;
            Ok(Vec::new())
        }
        // A real `Alt`+key press, which is how terminals without the enhanced
        // keyboard protocol encode it.
        _ => {
            key.modifiers |= KeyModifiers::ALT;
            Ok(vec![Event::Key(key)])
        }
    }
}

/// Read the body of a `CSI` sequence and turn it into the event it encodes.
fn read_csi(source: &mut impl EventSource, deadline: Instant) -> io::Result<Vec<Event>> {
    let mut body = String::new();
    loop {
        let Some(event) = next_before(source, deadline)? else {
            // Truncated sequence: drop it rather than replay its bytes as keys.
            return Ok(Vec::new());
        };
        // A non-character event ends the sequence. Keep the event, drop the
        // partial sequence.
        let Event::Key(key) = event else {
            return Ok(vec![event]);
        };
        let KeyCode::Char(ch) = key.code else {
            return Ok(vec![Event::Key(key)]);
        };

        if is_csi_final(ch) {
            if let Some(event) = parse_csi(&body, ch) {
                return Ok(vec![event]);
            }
            // Unrecognised. Most letters are legal `CSI` terminators, so `ESC [`
            // followed by a plain letter is far more likely a stray `ESC` plus a
            // real keypress than a control sequence — losing `q` there would
            // read as the app ignoring quit. Only drop the character when it
            // could actually terminate a report a terminal sends us.
            if body.is_empty() && !is_report_final(ch) {
                return Ok(vec![Event::Key(key)]);
            }
            return Ok(Vec::new());
        }
        if !is_csi_body(ch) || body.len() >= MAX_SEQUENCE_BYTES {
            // Cannot belong to this sequence, so it is a genuine keypress.
            return Ok(vec![Event::Key(key)]);
        }
        body.push(ch);
    }
}

/// `SS3` (`ESC O x`) encodes one key in the single byte that follows.
fn read_ss3(source: &mut impl EventSource, deadline: Instant) -> io::Result<Vec<Event>> {
    let Some(event) = next_before(source, deadline)? else {
        return Ok(Vec::new());
    };
    let Event::Key(key) = event else {
        return Ok(vec![event]);
    };
    let KeyCode::Char(ch) = key.code else {
        return Ok(vec![Event::Key(key)]);
    };
    let code = match ch {
        'A' => KeyCode::Up,
        'B' => KeyCode::Down,
        'C' => KeyCode::Right,
        'D' => KeyCode::Left,
        'H' => KeyCode::Home,
        'F' => KeyCode::End,
        'P' => KeyCode::F(1),
        'Q' => KeyCode::F(2),
        'R' => KeyCode::F(3),
        'S' => KeyCode::F(4),
        _ => return Ok(vec![Event::Key(key)]),
    };
    Ok(vec![Event::Key(KeyEvent::new(code, KeyModifiers::NONE))])
}

/// Swallow a control string up to `ST` (`ESC \`) or `BEL`.
fn consume_control_string(source: &mut impl EventSource, deadline: Instant) -> io::Result<()> {
    let mut saw_esc = false;
    loop {
        let Some(event) = next_before(source, deadline)? else {
            return Ok(());
        };
        let Event::Key(key) = event else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => saw_esc = true,
            KeyCode::Char('\\') if saw_esc => return Ok(()),
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
            _ => saw_esc = false,
        }
    }
}

fn next_before(source: &mut impl EventSource, deadline: Instant) -> io::Result<Option<Event>> {
    let budget = deadline
        .saturating_duration_since(Instant::now())
        .min(CONTINUATION_WINDOW);
    source.next(budget)
}

/// Parameter and intermediate bytes, i.e. everything legal inside a `CSI` body.
fn is_csi_body(ch: char) -> bool {
    matches!(ch, '\u{20}'..='\u{3f}')
}

fn is_csi_final(ch: char) -> bool {
    matches!(ch, '\u{40}'..='\u{7e}')
}

/// Final bytes of the parameterless reports a terminal actually sends us: focus
/// in/out, `CSI Z` for back-tab, the cursor keys, and legacy X10 mouse. Anything
/// else with an empty parameter list is treated as a real keypress instead.
fn is_report_final(ch: char) -> bool {
    matches!(
        ch,
        'I' | 'O' | 'Z' | 'M' | 'A' | 'B' | 'C' | 'D' | 'H' | 'F'
    )
}

/// Modifier mask from the enhanced-keyboard/xterm encoding, which is 1-based and
/// may carry an event type after a `:`.
fn parse_modifiers(value: &str) -> KeyModifiers {
    let mask = value
        .split(':')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(1)
        .saturating_sub(1);
    let mut modifiers = KeyModifiers::NONE;
    modifiers.set(KeyModifiers::SHIFT, mask & 1 != 0);
    modifiers.set(KeyModifiers::ALT, mask & 2 != 0);
    modifiers.set(KeyModifiers::CONTROL, mask & 4 != 0);
    modifiers.set(KeyModifiers::SUPER, mask & 8 != 0);
    modifiers.set(KeyModifiers::HYPER, mask & 16 != 0);
    modifiers.set(KeyModifiers::META, mask & 32 != 0);
    modifiers
}

/// Rebuild the event a `CSI <body> <final>` sequence stands for.
fn parse_csi(body: &str, final_byte: char) -> Option<Event> {
    if body.is_empty() {
        match final_byte {
            'I' => return Some(Event::FocusGained),
            'O' => return Some(Event::FocusLost),
            'Z' => {
                return Some(Event::Key(KeyEvent::new(
                    KeyCode::BackTab,
                    KeyModifiers::SHIFT,
                )));
            }
            _ => {}
        }
    }

    let modifiers = body
        .rsplit(';')
        .next()
        .filter(|_| body.contains(';'))
        .map(parse_modifiers)
        .unwrap_or(KeyModifiers::NONE);

    let code = match final_byte {
        'A' => KeyCode::Up,
        'B' => KeyCode::Down,
        'C' => KeyCode::Right,
        'D' => KeyCode::Left,
        'H' => KeyCode::Home,
        'F' => KeyCode::End,
        'u' => {
            let codepoint = body.split(';').next()?.split(':').next()?.parse().ok()?;
            match codepoint {
                9 => KeyCode::Tab,
                13 => KeyCode::Enter,
                27 => KeyCode::Esc,
                127 => KeyCode::Backspace,
                value => KeyCode::Char(char::from_u32(value)?),
            }
        }
        '~' => match body.split(';').next()? {
            "1" | "7" => KeyCode::Home,
            "2" => KeyCode::Insert,
            "3" => KeyCode::Delete,
            "4" | "8" => KeyCode::End,
            "5" => KeyCode::PageUp,
            "6" => KeyCode::PageDown,
            _ => return None,
        },
        _ => return None,
    };

    Some(Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a scripted list of events; `None` entries stand for a timeout.
    struct Scripted(std::collections::VecDeque<Option<Event>>);

    impl Scripted {
        fn new(items: Vec<Option<Event>>) -> Self {
            Self(items.into_iter().collect())
        }

        /// Everything the state machine yields for the scripted input, assuming
        /// the leading `Esc` has already been taken.
        fn resolve(items: Vec<Option<Event>>) -> Vec<Event> {
            let mut source = Scripted::new(items);
            resolve_escape(&mut source).expect("resolve")
        }
    }

    impl EventSource for Scripted {
        fn next(&mut self, _timeout: Duration) -> io::Result<Option<Event>> {
            Ok(self.0.pop_front().flatten())
        }
    }

    fn ch(c: char) -> Option<Event> {
        Some(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )))
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn bare_escape_stays_an_escape() {
        assert_eq!(Scripted::resolve(vec![None]), vec![key(KeyCode::Esc)]);
    }

    #[test]
    fn split_focus_report_never_reaches_shortcut_handling() {
        // `ESC [ I` arriving as three key events must become a focus event, not
        // the `I` that opens the interactive rebase picker.
        assert_eq!(
            Scripted::resolve(vec![ch('['), ch('I')]),
            vec![Event::FocusGained]
        );
    }

    #[test]
    fn split_arrow_key_is_reassembled() {
        assert_eq!(
            Scripted::resolve(vec![ch('['), ch('A')]),
            vec![key(KeyCode::Up)]
        );
    }

    #[test]
    fn split_modified_key_keeps_its_modifier() {
        assert_eq!(
            Scripted::resolve(vec![ch('['), ch('4'), ch('9'), ch(';'), ch('9'), ch('u')]),
            vec![Event::Key(KeyEvent {
                code: KeyCode::Char('1'),
                modifiers: KeyModifiers::SUPER,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            })]
        );
    }

    #[test]
    fn escape_then_letter_is_alt_modified() {
        assert_eq!(
            Scripted::resolve(vec![ch('x')]),
            vec![Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::ALT
            ))]
        );
    }

    #[test]
    fn truncated_sequence_is_dropped_rather_than_leaked() {
        // Payload bytes must never be replayed as keypresses.
        assert_eq!(Scripted::resolve(vec![ch('['), ch('3'), None]), vec![]);
    }

    #[test]
    fn key_that_cannot_continue_a_sequence_is_still_delivered() {
        // `q` happens to be a legal `CSI` terminator, but `ESC [ q` is not a
        // report any terminal sends, so quit must still get through.
        assert_eq!(
            Scripted::resolve(vec![ch('['), ch('q')]),
            vec![key(KeyCode::Char('q'))]
        );
    }

    #[test]
    fn unmodelled_terminal_report_is_dropped_not_typed() {
        // A device-attributes reply (`ESC [ ? 6 2 ; 2 2 c`) has to vanish rather
        // than land in a text field.
        assert_eq!(
            Scripted::resolve(vec![
                ch('['),
                ch('?'),
                ch('6'),
                ch('2'),
                ch(';'),
                ch('2'),
                ch('2'),
                ch('c'),
            ]),
            vec![]
        );
    }

    #[test]
    fn interior_key_press_is_not_swallowed_by_a_stalled_sequence() {
        // Enter cannot appear inside a `CSI` body, so it is a real keypress.
        assert_eq!(
            Scripted::resolve(vec![ch('['), ch('1'), Some(key(KeyCode::Enter))]),
            vec![key(KeyCode::Enter)]
        );
    }

    #[test]
    fn typed_text_after_a_stray_escape_survives() {
        // Non-ASCII input is neither a parameter nor a terminator byte.
        assert_eq!(
            Scripted::resolve(vec![ch('['), ch('é')]),
            vec![key(KeyCode::Char('é'))]
        );
    }

    #[test]
    fn non_key_event_after_introducer_is_preserved() {
        let resize = Event::Resize(80, 24);
        assert_eq!(
            Scripted::resolve(vec![ch('['), Some(resize.clone())]),
            vec![resize]
        );
    }

    #[test]
    fn control_string_payload_is_swallowed_entirely() {
        // An OSC reply must not spray its text into the UI.
        assert_eq!(
            Scripted::resolve(vec![
                ch(']'),
                ch('1'),
                ch('1'),
                ch(';'),
                ch('r'),
                ch('g'),
                ch('b'),
                Some(key(KeyCode::Esc)),
                ch('\\'),
            ]),
            vec![]
        );
    }

    #[test]
    fn ss3_function_key_is_reassembled() {
        assert_eq!(
            Scripted::resolve(vec![ch('O'), ch('B')]),
            vec![key(KeyCode::Down)]
        );
    }

    #[test]
    fn csi_grammar_classifies_parameter_and_final_bytes() {
        assert!(is_csi_body('3'));
        assert!(is_csi_body(';'));
        assert!(!is_csi_body('q'));
        assert!(is_csi_final('I'));
        assert!(is_csi_final('u'));
        assert!(!is_csi_final('3'));
    }
}
