use neotime_companion::{CallKind as PhoneCallKind, PhoneCall};
use neotime_runtime::EffectKind;

use super::*;

#[test]
fn a_call_becomes_the_call_event_with_its_caller() {
    let kinds = [
        (PhoneCallKind::Incoming, CallKind::Incoming),
        (PhoneCallKind::Answered, CallKind::Answered),
        (PhoneCallKind::Ended, CallKind::Ended),
    ];
    for (phone_kind, kind) in kinds {
        let call = PhoneCall {
            kind: phone_kind,
            caller: "Sam".try_into().unwrap(),
        };
        assert_eq!(call_event(&call), PushedEvent::Call { kind, caller: "Sam" });
    }
}

#[test]
fn phone_effects_send_their_lines() {
    assert_eq!(
        line_for_effect(EffectKind::MusicControl, 1).unwrap(),
        "{\"t\":\"music\",\"n\":\"pause\"}\n"
    );
    assert_eq!(
        line_for_effect(EffectKind::CallControl, 1).unwrap(),
        "{\"t\":\"call\",\"n\":\"REJECT\"}\n"
    );
    assert_eq!(
        line_for_effect(EffectKind::FindPhone, 1).unwrap(),
        "{\"t\":\"findPhone\",\"n\":true}\n"
    );
    assert_eq!(
        line_for_effect(EffectKind::RequestMusicState, 0).unwrap(),
        "{\"t\":\"request\",\"n\":\"music\"}\n"
    );
    assert_eq!(
        line_for_effect(EffectKind::RequestNavigationState, 0).unwrap(),
        "{\"t\":\"request\",\"n\":\"nav\"}\n"
    );
    assert_eq!(line_for_effect(EffectKind::MusicControl, 99), None);
    assert_eq!(line_for_effect(EffectKind::Vibrate, 30), None);
}
