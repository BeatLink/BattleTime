// Turns what the device reports into messages for the watch: notifications, calls, their summaries and music.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::device::{CallState, Media, PhoneEvent};
use super::settings::CoreSettings;

// ============================================================================
// Limits
// ============================================================================

// Longest notification text sent, since the watch keeps notifications in a few kilobytes of heap.
pub const NOTIFY_TITLE_LIMIT: usize = 60;
pub const NOTIFY_BODY_LIMIT: usize = 300;

// Phone notifications take ids below 2^31 and the watch's own apps take negative ones, so summaries count up from 2^31.
const FIRST_SUMMARY_ID: i64 = 1 << 31;

pub fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit - 1).collect();
    format!("{kept}…")
}

// ============================================================================
// Music
// ============================================================================

// A whole number of seconds goes out as an integer, which is what the watch's parser expects.
fn seconds(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

// The music lines the watch gets for what is playing.
pub fn music_messages(media: &Media) -> Vec<Value> {
    let mut info = json!({ "t": "musicinfo" });
    let mut state = json!({ "t": "musicstate", "state": if media.playing { "play" } else { "pause" } });
    for (key, value) in [
        ("artist", &media.artist),
        ("album", &media.album),
        ("track", &media.track),
    ] {
        if let Some(value) = value {
            info[key] = json!(value);
        }
    }
    if let Some(duration) = media.duration_sec {
        info["dur"] = seconds(duration);
    }
    if let Some(position) = media.position_sec {
        state["position"] = seconds(position);
    }
    vec![info, state]
}

// The answer to a watch app asking for the music or navigation again; navigation is always none.
pub fn answer_state_request(request: &Value, media: Option<&Media>, forward_music: bool) -> Vec<Value> {
    if request["n"] == "nav" {
        return vec![json!({ "t": "nav" })];
    }
    match media {
        Some(media) if forward_music => music_messages(media),
        _ => Vec::new(),
    }
}

// ============================================================================
// Call summaries
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallOutcome {
    Missed,
    Ignored,
    Accepted,
}

impl CallOutcome {
    fn text(self) -> &'static str {
        match self {
            CallOutcome::Missed => "Missed call",
            CallOutcome::Ignored => "Ignored call",
            CallOutcome::Accepted => "Accepted call",
        }
    }
}

struct RingingCall {
    caller: String,
    outcome: CallOutcome,
}

// Follows an incoming call so that, once it ends, the watch keeps a notification saying how it went.
pub struct CallSummary {
    ringing: Option<RingingCall>,
    next_id: i64,
}

impl Default for CallSummary {
    fn default() -> Self {
        CallSummary {
            ringing: None,
            next_id: FIRST_SUMMARY_ID,
        }
    }
}

impl CallSummary {
    pub fn note_call(&mut self, state: CallState, name: Option<&str>, number: Option<&str>) -> Option<Value> {
        match state {
            CallState::Incoming => {
                let caller = [name, number]
                    .into_iter()
                    .flatten()
                    .find(|text| !text.is_empty())
                    .unwrap_or("Unknown caller");
                self.ringing = Some(RingingCall {
                    caller: caller.to_string(),
                    outcome: CallOutcome::Missed,
                });
                None
            }
            CallState::Start => {
                if let Some(call) = &mut self.ringing {
                    call.outcome = CallOutcome::Accepted;
                }
                None
            }
            CallState::Outgoing => {
                self.ringing = None;
                None
            }
            CallState::End => {
                let call = self.ringing.take()?;
                let id = self.next_id;
                self.next_id += 1;
                Some(
                    json!({ "t": "notify", "id": id, "src": "Phone", "title": call.caller, "body": call.outcome.text() }),
                )
            }
        }
    }

    // A call rejected or muted on the watch counts as ignored, unless it is answered after all.
    pub fn note_watch_answer(&mut self, command: &str) {
        if let Some(call) = &mut self.ringing {
            if command != "ACCEPT" && call.outcome == CallOutcome::Missed {
                call.outcome = CallOutcome::Ignored;
            }
        }
    }
}

// ============================================================================
// Forwarding
// ============================================================================

#[derive(Default)]
pub struct Forwarder {
    // An app that updates a notification posts it again, so unchanged repeats are not sent twice.
    sent: HashMap<i64, String>,
    pub calls: CallSummary,
}

impl Forwarder {
    // Whether the event needs to know if the device is in Do Not Disturb.
    pub fn needs_quiet(event: &PhoneEvent, settings: &CoreSettings) -> bool {
        settings.respect_do_not_disturb
            && matches!(event, PhoneEvent::Notification { .. } | PhoneEvent::Call { .. })
    }

    // The messages for the watch, given whether the watch can take them and whether the device is quiet.
    pub fn messages(
        &mut self,
        event: &PhoneEvent,
        settings: &CoreSettings,
        ready: bool,
        quiet: bool,
    ) -> Vec<Value> {
        // Calls are followed even while the watch is away, so a summary never names the wrong outcome.
        let summary = match event {
            PhoneEvent::Call { state, name, number } => {
                self.calls.note_call(*state, name.as_deref(), number.as_deref())
            }
            _ => None,
        };
        if !ready {
            return Vec::new();
        }
        let mut messages = Vec::new();
        match event {
            PhoneEvent::Notification { id, app, title, body } => {
                if settings.forward_notifications && !quiet {
                    let title = clip(title, NOTIFY_TITLE_LIMIT);
                    let body = clip(body, NOTIFY_BODY_LIMIT);
                    let key = format!("{app}\n{title}\n{body}");
                    if self.sent.get(id) != Some(&key) {
                        self.sent.insert(*id, key);
                        messages.push(
                            json!({ "t": "notify", "id": id, "src": app, "title": title, "body": body }),
                        );
                    }
                }
            }
            PhoneEvent::NotificationRemoved { id } => {
                if settings.forward_notifications && self.sent.remove(id).is_some() {
                    messages.push(json!({ "t": "notify-", "id": id }));
                }
            }
            PhoneEvent::Call { state, name, number } => {
                if settings.forward_calls && !(*state == CallState::Incoming && quiet) {
                    let mut call = json!({ "t": "call", "cmd": state.as_str() });
                    if let Some(name) = name {
                        call["name"] = json!(name);
                    }
                    if let Some(number) = number {
                        call["number"] = json!(number);
                    }
                    messages.push(call);
                }
                if let Some(summary) = summary {
                    if settings.forward_notifications && !quiet {
                        messages.push(summary);
                    }
                }
            }
            PhoneEvent::Media(media) => {
                if settings.forward_music {
                    messages.extend(music_messages(media));
                }
            }
            PhoneEvent::SosStopped => {}
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(id: i64, title: &str) -> PhoneEvent {
        PhoneEvent::Notification {
            id,
            app: "Mail".into(),
            title: title.into(),
            body: "Body".into(),
        }
    }

    fn call(state: CallState) -> PhoneEvent {
        PhoneEvent::Call {
            state,
            name: Some("Ann".into()),
            number: None,
        }
    }

    #[test]
    fn long_text_is_clipped_with_an_ellipsis() {
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("abcd", 4), "abcd");
        assert_eq!(clip("ééééé", 3), "éé…");
    }

    #[test]
    fn a_notification_is_sent_once_until_it_changes_and_then_removed() {
        let mut forwarder = Forwarder::default();
        let settings = CoreSettings::default();

        let first = forwarder.messages(&notification(7, "Hi"), &settings, true, false);
        let repeat = forwarder.messages(&notification(7, "Hi"), &settings, true, false);
        let changed = forwarder.messages(&notification(7, "Hello"), &settings, true, false);
        let removed = forwarder.messages(&PhoneEvent::NotificationRemoved { id: 7 }, &settings, true, false);
        let removed_again =
            forwarder.messages(&PhoneEvent::NotificationRemoved { id: 7 }, &settings, true, false);

        assert_eq!(
            first,
            vec![json!({ "t": "notify", "id": 7, "src": "Mail", "title": "Hi", "body": "Body" })]
        );
        assert!(repeat.is_empty());
        assert_eq!(changed[0]["title"], "Hello");
        assert_eq!(removed, vec![json!({ "t": "notify-", "id": 7 })]);
        assert!(removed_again.is_empty());
    }

    #[test]
    fn nothing_goes_out_while_quiet_turned_off_or_not_ready() {
        let mut forwarder = Forwarder::default();
        let off = CoreSettings {
            forward_notifications: false,
            ..CoreSettings::default()
        };

        assert!(forwarder
            .messages(&notification(1, "a"), &CoreSettings::default(), true, true)
            .is_empty());
        assert!(forwarder
            .messages(&notification(2, "a"), &off, true, false)
            .is_empty());
        assert!(forwarder
            .messages(&notification(3, "a"), &CoreSettings::default(), false, false)
            .is_empty());
        assert!(Forwarder::needs_quiet(
            &notification(1, "a"),
            &CoreSettings::default()
        ));
        assert!(!Forwarder::needs_quiet(
            &PhoneEvent::Media(Media::default()),
            &CoreSettings::default()
        ));
    }

    #[test]
    fn a_missed_call_rings_and_leaves_a_summary() {
        let mut forwarder = Forwarder::default();
        let settings = CoreSettings::default();

        let ringing = forwarder.messages(&call(CallState::Incoming), &settings, true, false);
        let ended = forwarder.messages(&call(CallState::End), &settings, true, false);

        assert_eq!(
            ringing,
            vec![json!({ "t": "call", "cmd": "incoming", "name": "Ann" })]
        );
        assert_eq!(ended[0], json!({ "t": "call", "cmd": "end", "name": "Ann" }));
        assert_eq!(
            ended[1],
            json!({ "t": "notify", "id": FIRST_SUMMARY_ID, "src": "Phone", "title": "Ann", "body": "Missed call" })
        );
    }

    #[test]
    fn a_quiet_phone_keeps_the_ring_off_the_watch_but_still_follows_the_call() {
        let mut forwarder = Forwarder::default();
        let settings = CoreSettings::default();

        assert!(forwarder
            .messages(&call(CallState::Incoming), &settings, true, true)
            .is_empty());
        forwarder.messages(&call(CallState::Start), &settings, false, false);
        let ended = forwarder.messages(&call(CallState::End), &settings, true, false);

        assert_eq!(ended[1]["body"], "Accepted call");
    }

    #[test]
    fn a_call_turned_down_on_the_watch_is_ignored_not_missed() {
        let mut summary = CallSummary::default();
        summary.note_call(CallState::Incoming, None, Some("123"));
        summary.note_watch_answer("REJECT");
        let ended = summary.note_call(CallState::End, None, None).unwrap();
        assert_eq!(ended["title"], "123");
        assert_eq!(ended["body"], "Ignored call");
        assert_eq!(summary.note_call(CallState::End, None, None), None);
    }

    #[test]
    fn an_outgoing_call_leaves_no_summary() {
        let mut summary = CallSummary::default();
        summary.note_call(CallState::Incoming, None, None);
        summary.note_call(CallState::Outgoing, None, None);
        assert_eq!(summary.note_call(CallState::End, None, None), None);
    }

    #[test]
    fn music_is_sent_as_info_and_state() {
        let media = Media {
            playing: true,
            track: Some("Song".into()),
            duration_sec: Some(180.0),
            position_sec: Some(3.0),
            ..Media::default()
        };
        let mut forwarder = Forwarder::default();

        let messages = forwarder.messages(
            &PhoneEvent::Media(media.clone()),
            &CoreSettings::default(),
            true,
            false,
        );

        assert_eq!(
            messages,
            vec![
                json!({ "t": "musicinfo", "track": "Song", "dur": 180 }),
                json!({ "t": "musicstate", "state": "play", "position": 3 }),
            ]
        );
        assert_eq!(
            answer_state_request(&json!({ "t": "request", "n": "music" }), Some(&media), false),
            Vec::<Value>::new()
        );
        assert_eq!(
            answer_state_request(&json!({ "t": "request", "n": "nav" }), None, true),
            vec![json!({ "t": "nav" })]
        );
        assert_eq!(
            answer_state_request(&json!({ "t": "request", "n": "music" }), Some(&media), true).len(),
            2
        );
    }
}
