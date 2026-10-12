//! The phone as apps see it: its calls, music and weather delivered as events, and the lines an
//! app's phone effects send back, such as `music_control`.

use neotime_companion::{PhoneCall, PhoneChanges, ShortLine};
use neotime_runtime::{EffectKind, Value};

use crate::{AppHost, CallKind, PushedEvent, System};

/// The `call` event for a call from the phone.
pub fn call_event(call: &PhoneCall) -> PushedEvent<'_> {
    let kind = match call.kind {
        neotime_companion::CallKind::Incoming => CallKind::Incoming,
        neotime_companion::CallKind::Answered => CallKind::Answered,
        neotime_companion::CallKind::Ended => CallKind::Ended,
    };
    PushedEvent::Call {
        kind,
        caller: &call.caller,
    }
}

/// Tells the open app of a call and of new music or weather, which the system already reads back.
pub fn tell_app(
    host: &mut AppHost,
    call: Option<&PhoneCall>,
    changes: PhoneChanges,
    now: Value,
    system: &mut impl System,
) {
    if let Some(call) = call {
        host.deliver(call_event(call), now, system);
    }
    if changes.music {
        host.deliver(PushedEvent::Music, now, system);
    }
    if changes.weather {
        host.deliver(PushedEvent::Weather, now, system);
    }
}

/// The line an effect sends the phone, or `None` for an effect that sends none.
pub fn line_for_effect(kind: EffectKind, first_argument: Value) -> Option<ShortLine> {
    match kind {
        EffectKind::MusicControl => neotime_companion::music_control_line(first_argument),
        EffectKind::CallControl => neotime_companion::call_control_line(first_argument),
        EffectKind::FindPhone => Some(neotime_companion::find_phone_line(first_argument != 0)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "phone_tests.rs"]
mod phone_tests;
