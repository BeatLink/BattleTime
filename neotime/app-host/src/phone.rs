//! The phone as apps see it: its calls, music, weather and navigation delivered as events, and the
//! lines an app's phone effects send back, such as `music_control` and `send_to_phone`.

use neotime_companion::{PhoneCall, PhoneChanges, ShortLine};
use neotime_runtime::{EffectKind, Value};

use alloc::string::String;

use crate::{AppEffect, AppHost, CallKind, PushedEvent, System};

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

/// Tells the open app of a call and of new music, weather or navigation, which the system already reads back.
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
    if changes.navigation {
        host.deliver(PushedEvent::Navigation, now, system);
    }
}

/// The line an effect sends the phone, or `None` for an effect that sends none.
pub fn line_for_effect(kind: EffectKind, first_argument: Value) -> Option<ShortLine> {
    match kind {
        EffectKind::MusicControl => neotime_companion::music_control_line(first_argument),
        EffectKind::CallControl => neotime_companion::call_control_line(first_argument),
        EffectKind::FindPhone => Some(neotime_companion::find_phone_line(first_argument != 0)),
        EffectKind::RequestMusicState => Some(neotime_companion::state_request_line(false)),
        EffectKind::RequestNavigationState => Some(neotime_companion::state_request_line(true)),
        _ => None,
    }
}

/// The `app` line a `send_to_phone` effect sends, or `None` for another effect or one that cannot be sent.
pub fn app_line_for_effect(effect: &AppEffect) -> Option<String> {
    match effect.kind {
        EffectKind::SendToPhone => neotime_companion::app_message_line(effect.text(0)?, effect.text(1)?),
        _ => None,
    }
}

#[cfg(test)]
#[path = "phone_tests.rs"]
mod phone_tests;
