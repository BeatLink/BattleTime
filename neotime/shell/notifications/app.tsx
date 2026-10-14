// Notifications, from InfiniTime's Notifications screen and wasp-os's notification list.
// The watch keeps the latest few from the phone and from apps; this lists them newest first, each
// with its title and the start of its text. A tap reads one in full and Dismiss removes it.
// Opening the app, or the screen turning on while it is open, marks them all read.
// When a notification arrives the shell opens the preview, which shows the newest one and goes back
// by itself after 7 seconds unless the wearer taps it to read it in full.
// Each notification shows an icon for its kind: a call, a missed call, a text, an email and so on.

import {
  back,
  define_app,
  dismiss_notification,
  format_time,
  go,
  mark_notifications_read,
  notifications,
  now,
  on,
  range,
  signal,
} from "neotime";
import { icons } from "neotime/icons";
import { Bar, Button, Column, Icon, Label, List, Row, Screen, Scroll } from "neotime/widgets";

// ============================================================================
// Layout
// ============================================================================

/** As many as the watch keeps. */
const MOST_NOTIFICATIONS = 5;

const ROW_HEIGHT = 60;

/** Space around the list and between its rows. */
const SPACING = 8;

/** How long the preview stays before it goes back by itself. */
const PREVIEW_MILLISECONDS = 7000;

/** How often the preview's bar shrinks. */
const PREVIEW_STEP_MILLISECONDS = 250;

// ============================================================================
// State
// ============================================================================

/** The place in the list of the notification being read. */
const reading = signal(0);

/** When the preview goes back by itself, or 0 once it stays. */
const preview_closes_at = signal(0);

/** How long the preview has left, which the bar across its top shows. */
const preview_left = signal(0);

/** Whether the wearer tapped the preview to read the notification in full. */
const is_preview_open = signal(false);

// ============================================================================
// Handlers
// ============================================================================

function read(place: number) {
  reading.value = place;
  go("message");
}

/** Shows the newest notification for the preview's full time. */
function start_preview() {
  reading.value = 0;
  is_preview_open.value = false;
  preview_closes_at.value = now.value + PREVIEW_MILLISECONDS;
  preview_left.value = PREVIEW_MILLISECONDS;
}

function stop_preview() {
  preview_closes_at.value = 0;
}

/** Shrinks the preview's bar, and goes back once its time is up. */
function count_down_preview() {
  if (preview_closes_at.value === 0) return;
  preview_left.value = Math.max(0, preview_closes_at.value - now.value);
  if (preview_left.value === 0) {
    stop_preview();
    back();
  }
}

/** Starts the preview again for a newer notification while it is still counting down. */
function preview_newer() {
  if (preview_closes_at.value !== 0) start_preview();
}

function open_preview() {
  stop_preview();
  is_preview_open.value = true;
}

/** The icon for the notification at this place, which the watch picks from its app; an envelope past the end. */
function kind_icon(place: number) {
  return place < notifications.value.length ? notifications.value[place].icon : icons.notification;
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
          <Row padding={6} gap={8} align_items="center">
            <Icon icon={kind_icon(props.place)} size={20} color="accent" />
            <Column grow={1} gap={2}>
              <Label long_mode="scroll">{notifications.value[props.place].title}</Label>
              <Label font="small" color="muted" long_mode="clip">{notifications.value[props.place].body}</Label>
            </Column>
          </Row>
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
      <MessageText />
    </Scroll>
  );
}

function MessageText() {
  return (
    <>
      {reading.value < notifications.value.length && (
        <Column gap={6}>
          <Row gap={6} align_items="center">
            <Icon icon={kind_icon(reading.value)} size={20} color="accent" />
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
    </>
  );
}

/** The newest notification as it arrives: a tap reads it in full, and it goes back by itself once the bar runs out. */
function Preview() {
  return (
    <Screen onShow={start_preview} onHide={stop_preview}>
      {!is_preview_open.value && (
        <>
          <Bar height={4} value={preview_left.value} maximum={PREVIEW_MILLISECONDS} animate={PREVIEW_STEP_MILLISECONDS} />
          <Button grow={1} background="background" onClick={open_preview}>
            <Column grow={1} padding={SPACING} gap={6} justify="start">
              <Row gap={6} align_items="center">
                <Icon icon={kind_icon(0)} size={20} color="accent" />
                <Label font="small" color="muted" grow={1} long_mode="clip">
                  {notifications.value.length > 0 ? notifications.value[0].app : ""}
                </Label>
              </Row>
              <Label font="medium" long_mode="clip">{notifications.value.length > 0 ? notifications.value[0].title : ""}</Label>
              <Label>{notifications.value.length > 0 ? notifications.value[0].body : ""}</Label>
            </Column>
          </Button>
        </>
      )}
      {is_preview_open.value && (
        <Scroll grow={1} padding={SPACING}>
          <MessageText />
        </Scroll>
      )}
    </Screen>
  );
}

on("open", mark_notifications_read);
on("screen_on", mark_notifications_read);
on("tick", count_down_preview, { every: { milliseconds: PREVIEW_STEP_MILLISECONDS }, while_open: true });
on("notification", preview_newer, { while_open: true });

export default define_app({
  id: "notifications",
  name: "Notifications",
  icon: icons.notification,
  screens: { main: NotificationList, message: Message, preview: Preview },
});
