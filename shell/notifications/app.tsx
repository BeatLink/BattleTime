// Notifications, from InfiniTime's Notifications screen and wasp-os's notification list.
// The watch keeps the latest few from the phone and from apps; this lists them newest first, each
// with its title and the start of its text. A tap reads one in full and Dismiss removes it.
// Opening the app, or the screen turning on while it is open, marks them all read.

import { signal } from "@preact/signals";
import {
  back,
  define_app,
  dismiss_notification,
  format_time,
  go,
  mark_notifications_read,
  notifications,
  range,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, List, Row, Screen, Scroll } from "neotime/widgets";

// ============================================================================
// Layout
// ============================================================================

/** As many as the watch keeps. */
const MOST_NOTIFICATIONS = 5;

const ROW_HEIGHT = 60;

/** Space around the list and between its rows. */
const SPACING = 8;

// ============================================================================
// State
// ============================================================================

/** The place in the list of the notification being read. */
const reading = signal(0);

// ============================================================================
// Handlers
// ============================================================================

function read(place: number) {
  reading.value = place;
  go("message");
}

function dismiss() {
  dismiss_notification(notifications.value[reading.value].id);
  back();
}

// ============================================================================
// Screens
// ============================================================================

function NotificationRow(props: { place: number }) {
  return (
    <>
      {props.place < notifications.value.length && (
        <Button height={ROW_HEIGHT} onClick={() => read(props.place)}>
          <Column padding={6} gap={2}>
            <Label long_mode="scroll">{notifications.value[props.place].title}</Label>
            <Label font="small" color="muted" long_mode="clip">{notifications.value[props.place].body}</Label>
          </Column>
        </Button>
      )}
    </>
  );
}

function NotificationList() {
  return (
    <Screen>
      {notifications.value.length === 0 && (
        <Column grow={1} align_items="center" justify="center" gap={12}>
          <Icon icon={icons.notification} size={64} color="muted" />
          <Label color="muted">No notifications</Label>
        </Column>
      )}
      {notifications.value.length > 0 && (
        <List grow={1} padding={SPACING} gap={SPACING}>
          {range(MOST_NOTIFICATIONS).map((place) => (
            <NotificationRow place={place} />
          ))}
        </List>
      )}
    </Screen>
  );
}

/** The notification being read, in full; nothing once it has gone from the list. */
function Message() {
  return (
    <Scroll padding={SPACING}>
      {reading.value < notifications.value.length && (
        <Column gap={6}>
          <Row gap={8}>
            <Label font="small" color="muted" grow={1} long_mode="clip">{notifications.value[reading.value].app}</Label>
            <Label font="small" color="muted">{format_time(notifications.value[reading.value].time, "clock")}</Label>
          </Row>
          <Label font="large">{notifications.value[reading.value].title}</Label>
          <Label>{notifications.value[reading.value].body}</Label>
          <Button height={48} background="danger" onClick={dismiss}>
            <Label text_align="center">Dismiss</Label>
          </Button>
        </Column>
      )}
    </Scroll>
  );
}

export default define_app({
  id: "notifications",
  name: "Notifications",
  icon: icons.notification,
  permissions: ["notifications"],
  screens: { main: NotificationList, message: Message },
  onOpen: mark_notifications_read,
  onScreenOn: mark_notifications_read,
});
