//! The notifications the watch keeps, newest first: the latest few from the phone and from apps.
//!
//! The list holds at most `MOST_KEPT`, and a new one past that pushes out the oldest. Each one's
//! text waits on the heap, so an empty list costs almost nothing. A notification with the id of
//! one already kept replaces it, since the phone sends an update to a notification that way.
//! Apps read the list through the `notifications` and `unread_notifications` system values, and
//! change it with `notify`, `dismiss_notification` and `mark_notifications_read`.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;

use neotime_app_host::AppEffect;
use neotime_companion::PhoneNotification;
use neotime_runtime::{EffectKind, SystemValue, Value, NONE};

/// The most notifications kept; InfiniTime keeps as many.
pub const MOST_KEPT: usize = 5;

/// The id of the first notification an app makes; the next ones count down, so none takes a phone's id.
const FIRST_APP_NOTIFICATION_ID: Value = -1;

// ============================================================================
// The list
// ============================================================================

struct Kept {
    notification: Box<PhoneNotification>,
    received_at: Value,
    is_read: bool,
}

pub struct NotificationList {
    /// Oldest first, so a new one goes on the end.
    kept: heapless::Vec<Kept, MOST_KEPT>,
    next_app_notification_id: Value,
}

impl Default for NotificationList {
    fn default() -> Self {
        NotificationList::new()
    }
}

impl NotificationList {
    pub const fn new() -> Self {
        NotificationList {
            kept: heapless::Vec::new(),
            next_app_notification_id: FIRST_APP_NOTIFICATION_ID,
        }
    }

    /// Keeps a notification as the newest and unread, in place of any with its id.
    pub fn add(&mut self, notification: Box<PhoneNotification>, now: Value) {
        self.kept.retain(|kept| kept.notification.id != notification.id);
        if self.kept.is_full() {
            self.kept.remove(0);
        }
        let _ = self.kept.push(Kept {
            notification,
            received_at: now,
            is_read: false,
        });
    }

    /// Keeps a notification an app made, its text cut to fit, and gives the id it was given.
    pub fn add_from_app(&mut self, app: &str, title: &str, body: &str, now: Value) -> Value {
        let id = self.next_app_notification_id;
        self.next_app_notification_id -= 1;
        let notification = PhoneNotification {
            id,
            source: cut_to_fit(app),
            title: cut_to_fit(title),
            body: cut_to_fit(body),
        };
        self.add(Box::new(notification), now);
        id
    }

    /// Removes the notification with this id; false when none has it.
    pub fn dismiss(&mut self, id: Value) -> bool {
        let count_before = self.kept.len();
        self.kept.retain(|kept| kept.notification.id != id);
        self.kept.len() != count_before
    }

    /// Marks every notification read; false when all already were.
    pub fn mark_all_read(&mut self) -> bool {
        let mut changed = false;
        for kept in self.kept.iter_mut() {
            changed |= !kept.is_read;
            kept.is_read = true;
        }
        changed
    }

    pub fn len(&self) -> usize {
        self.kept.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    pub fn unread_count(&self) -> usize {
        self.kept.iter().filter(|kept| !kept.is_read).count()
    }

    /// The notification at this place, 0 being the newest.
    pub fn get(&self, place: usize) -> Option<&PhoneNotification> {
        self.kept_at(place as Value).map(|kept| &*kept.notification)
    }

    fn kept_at(&self, place: Value) -> Option<&Kept> {
        let place = usize::try_from(place)
            .ok()
            .filter(|&place| place < self.kept.len())?;
        self.kept.get(self.kept.len() - 1 - place)
    }
}

/// Text cut at the last whole character that fits.
pub fn cut_to_fit<const BYTES: usize>(text: &str) -> heapless::String<BYTES> {
    let mut end = text.len().min(BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut cut = heapless::String::new();
    let _ = cut.push_str(&text[..end]);
    cut
}

// ============================================================================
// What apps ask for
// ============================================================================

impl NotificationList {
    /// Carries out `notify`, `dismiss_notification` or `mark_notifications_read`; true when the list changed.
    ///
    /// `app` is the name of the app asking, which a notification it makes shows as its source.
    pub fn carry_out(&mut self, effect: &AppEffect, app: &str, now: Value) -> bool {
        match effect.kind {
            EffectKind::Notify => {
                let title = effect.text(0).unwrap_or("");
                let body = effect.text(1).unwrap_or("");
                self.add_from_app(app, title, body, now);
                true
            }
            EffectKind::DismissNotification => self.dismiss(effect.number(0)),
            EffectKind::MarkNotificationsRead => self.mark_all_read(),
            _ => false,
        }
    }
}

// ============================================================================
// What apps read
// ============================================================================

impl NotificationList {
    /// `unread_notifications` and the length of `notifications`; `None` for any other value.
    pub fn read(&self, value: SystemValue) -> Option<Value> {
        match value {
            SystemValue::UnreadNotifications => Some(self.unread_count() as Value),
            SystemValue::NotificationCount => Some(self.len() as Value),
            _ => None,
        }
    }

    /// A kept notification's id or arrival time, or none past the end of the list.
    pub fn read_element(&self, value: SystemValue, place: Value) -> Value {
        let Some(kept) = self.kept_at(place) else {
            return NONE;
        };
        match value {
            SystemValue::NotificationId => kept.notification.id,
            SystemValue::NotificationTime => kept.received_at,
            _ => NONE,
        }
    }

    /// A kept notification's app, title or body, or empty past the end of the list.
    pub fn read_element_text(&self, value: SystemValue, place: Value) -> &str {
        let Some(kept) = self.kept_at(place) else {
            return "";
        };
        match value {
            SystemValue::NotificationApp => &kept.notification.source,
            SystemValue::NotificationTitle => &kept.notification.title,
            SystemValue::NotificationBody => &kept.notification.body,
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests;
