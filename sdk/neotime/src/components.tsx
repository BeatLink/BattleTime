// Ready-made components kept in the shared library, built from the widgets in widgets.tsx.
// They are written in the same subset as apps, so the compiler can build the library from them.

import type { ComponentChildren } from "preact";
import type { Signal } from "@preact/signals";
import {
  battery_percent,
  current_weather,
  format_temperature,
  heart_rate,
  is_charging,
  is_heart_rate_running,
  is_phone_connected,
  next_alarm,
  range,
  settings,
  steps_today,
  unread_notifications,
  type ChangeEvent,
  type Duration,
  type TimeOfDay,
  SECOND,
} from "./index.ts";
import { icons, type LibraryIcon } from "./icons.ts";
import {
  Badge,
  BatteryGauge,
  Button,
  Column,
  Icon,
  Label,
  Roller,
  Row,
  ConfirmBox,
  DayButtons,
  DurationRollers,
  NumberRoller,
  Stack,
  StatusRow,
  TimeRollers,
  type CommonProps,
  type FlexProps,
} from "./widgets.tsx";

// ============================================================================
// Panels and rows
// ============================================================================

export type CardProps = FlexProps;

/** A rounded panel, the standard block of an app's layout. */
export function Card(props: CardProps) {
  return (
    <Column background="surface" radius={12} padding={10} gap={props.gap ?? 4} {...props}>
      {props.children}
    </Column>
  );
}

export interface HeaderProps {
  title: string;
  icon?: LibraryIcon;
  color?: CommonProps["color"];
}

/** A screen's title row, with an optional icon. */
export function Header(props: HeaderProps) {
  return (
    <Row gap={8} height={36} padding={4}>
      {props.icon && <Icon icon={props.icon} size={28} color={props.color ?? "accent"} />}
      <Label color={props.color ?? "accent"}>{props.title}</Label>
    </Row>
  );
}

// ============================================================================
// Scroll bar
// ============================================================================

/** The bar runs down the right edge, two pixels clear of content padded by 4 or more. */
const SCROLL_BAR_X = 238;
const SCROLL_BAR_WIDTH = 2;
const SCROLL_BAR_HEIGHT = 240;
/** Rows left clear at each end of a page's segment, so the segments stand apart. */
const SCROLL_BAR_SEGMENT_INSET = 2;

export interface ScrollBarProps {
  /** The page on show, counted from 0. */
  page: number;
  /** How many pages there are; a single page draws no bar. */
  pages: number;
}

/**
 * One segment per page down the right edge, the page on show lighter than the rest, as wasp-os draws it.
 * Put it straight in a `Screen` without padding, since the watch places it inside the padding.
 */
export function ScrollBar(props: ScrollBarProps) {
  const pitch = Math.floor(SCROLL_BAR_HEIGHT / props.pages);
  return (
    <Stack x={SCROLL_BAR_X} y={0} width={SCROLL_BAR_WIDTH} height={SCROLL_BAR_HEIGHT}>
      {props.pages > 1 &&
        range(props.pages).map((page) => (
          <Column
            x={0}
            y={page * pitch + SCROLL_BAR_SEGMENT_INSET}
            width={SCROLL_BAR_WIDTH}
            height={pitch - 2 * SCROLL_BAR_SEGMENT_INSET}
            background={page === props.page ? "muted" : "surface"}
          />
        ))}
    </Stack>
  );
}

export interface BatteryIconProps {
  /** Shows the percentage beside the icon. */
  show_percent?: boolean;
  color?: CommonProps["color"];
  /** Draws the whole icon in `color`, without turning green while charging or red when low. */
  single_color?: boolean;
  /** Whether the charging bolt shows while charging; it does unless this is false. */
  show_bolt?: boolean;
}

/** The battery's charge and whether it is charging, drawn the way the system does, as one widget. */
export function BatteryIcon(props: BatteryIconProps) {
  return (
    <BatteryGauge
      value={battery_percent.value}
      charging={is_charging.value}
      color={props.color ?? "text"}
      single_color={props.single_color}
      show_bolt={props.show_bolt}
      show_percent={props.show_percent}
    />
  );
}

export interface StatusIconsProps extends CommonProps {
  /** The side of the icons. */
  icon_size?: number;
  gap?: number;
  /** The battery's colour, when it is not the icons'. */
  battery_color?: CommonProps["color"];
  /** Shows the notifications icon while any are unread; it does unless this is false. */
  show_notifications?: boolean;
  /** Shows the phone's icon while it is connected; it does unless this is false. */
  show_phone?: boolean;
  /** Shows the alarm icon while an alarm is set. */
  show_alarm?: boolean;
  show_percent?: boolean;
  single_color?: boolean;
  show_bolt?: boolean;
}

/** The status row the system shows: unread notifications, the phone, an alarm and the battery, as one widget. */
export function StatusIcons(props: StatusIconsProps) {
  const notifications = props.show_notifications !== false && unread_notifications.value > 0 ? 1 : 0;
  const phone = props.show_phone !== false && is_phone_connected.value ? 2 : 0;
  const alarm = props.show_alarm === true && next_alarm.value !== null ? 4 : 0;
  return (
    <StatusRow
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      padding={props.padding}
      align={props.align}
      color={props.color}
      border_color={props.battery_color ?? props.color ?? "text"}
      gap={props.gap ?? 6}
      icon_size={props.icon_size ?? 22}
      value={battery_percent.value}
      charging={is_charging.value}
      status_flags={notifications + phone + alarm}
      single_color={props.single_color}
      show_bolt={props.show_bolt}
      show_percent={props.show_percent}
    />
  );
}

export interface SystemBadgeProps extends CommonProps {
  icon_size?: number;
  gap?: number;
  /** The icon's colour, when it is not the text's. */
  icon_color?: CommonProps["color"];
}

/** Today's steps beside the steps icon, as one widget. */
export function StepsBadge(props: SystemBadgeProps) {
  return (
    <Badge
      icon={icons.steps}
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      padding={props.padding}
      align={props.align}
      color={props.color}
      font={props.font}
      gap={props.gap ?? 5}
      icon_size={props.icon_size ?? 20}
      recolor={props.icon_color}
    >
      {`${steps_today.value}`}
    </Badge>
  );
}

/** The heart rate beside the heart icon while it is measured, as one widget; the icon alone otherwise. */
export function HeartRateBadge(props: SystemBadgeProps) {
  return (
    <Badge
      icon={icons.heart}
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      padding={props.padding}
      align={props.align}
      color={props.color}
      font={props.font}
      gap={props.gap ?? 5}
      icon_size={props.icon_size ?? 20}
      recolor={props.icon_color}
    >
      {is_heart_rate_running.value ? `${heart_rate.value}` : ""}
    </Badge>
  );
}

export interface TemperatureBadgeProps extends SystemBadgeProps {
  /** The weather's icon, which the face chooses, such as a night icon after sunset. */
  icon: LibraryIcon;
}

/** The temperature in the wearer's units beside the weather's icon, as one widget. */
export function TemperatureBadge(props: TemperatureBadgeProps) {
  return (
    <Badge
      icon={props.icon}
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      padding={props.padding}
      align={props.align}
      color={props.color}
      font={props.font}
      gap={props.gap ?? 5}
      icon_size={props.icon_size ?? 20}
      recolor={props.icon_color}
    >
      {format_temperature(current_weather.value.temperature)}
    </Badge>
  );
}

export interface ConfirmDialogProps {
  /** The dialog shows while this is true, and closes itself on either button. */
  open: Signal<boolean>;
  question: string;
  confirm_text?: string;
  cancel_text?: string;
  onConfirm: () => void;
  onCancel?: () => void;
}

/** A question with two buttons, shown over the screen while a signal is true, as one widget. */
export function ConfirmDialog(props: ConfirmDialogProps) {
  if (!props.open.value) return null;
  return (
    <ConfirmBox
      x={0}
      y={0}
      width={240}
      height={240}
      background="#000000"
      text={`${props.cancel_text ?? "No"}\n${props.confirm_text ?? "Yes"}\n${props.question}`}
      onChange={(event) => {
        props.open.value = false;
        if (event.value === 1) {
          props.onConfirm();
        } else {
          props.onCancel?.();
        }
      }}
    />
  );
}

// ============================================================================
// Pickers
// ============================================================================

export interface NumberPickerProps extends CommonProps {
  value: Signal<number>;
  minimum: number;
  maximum: number;
  /** Digits to pad to, such as 2 for minutes. */
  digits?: number;
  onChange?: (event: ChangeEvent<number>) => void;
}

/** A roller set up for a whole number, as one widget. */
export function NumberPicker(props: NumberPickerProps) {
  return (
    <NumberRoller
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      grow={props.grow}
      font={props.font}
      color={props.color}
      value={props.value.value}
      minimum={props.minimum}
      maximum={props.maximum}
      digits={props.digits ?? 1}
      onChange={(event) => {
        props.value.value = event.value;
        props.onChange?.({ value: event.value });
      }}
    />
  );
}

export interface TimePickerProps {
  value: Signal<TimeOfDay>;
  onChange?: (event: ChangeEvent<TimeOfDay>) => void;
}

/** Rollers for an hour and a minute, following the 12 or 24 hour setting, as one widget. */
export function TimePicker(props: TimePickerProps) {
  return (
    <TimeRollers
      value={props.value.value.hour * 60 + props.value.value.minute}
      twelve_hour={!settings.uses_24_hour_clock.value}
      onChange={(event) => {
        props.value.value = { ...props.value.value, hour: Math.floor(event.value / 60), minute: event.value % 60 };
        props.onChange?.({ value: props.value.value });
      }}
    />
  );
}

export interface DurationPickerProps {
  value: Signal<Duration>;
  show_hours?: boolean;
  onChange?: (event: ChangeEvent<Duration>) => void;
}

/** Rollers for minutes and seconds, and hours if asked, as one widget. */
export function DurationPicker(props: DurationPickerProps) {
  return (
    <DurationRollers
      value={Math.floor(props.value.value / SECOND)}
      show_hours={props.show_hours === true}
      onChange={(event) => {
        props.value.value = event.value * SECOND;
        props.onChange?.({ value: props.value.value });
      }}
    />
  );
}

export interface WeekdayPickerProps extends CommonProps {
  /** The days set, Monday in bit 0. */
  value: Signal<number>;
  /** How many buttons in a row; all seven unless given. */
  columns?: number;
  gap?: number;
  onChange?: (event: ChangeEvent<number>) => void;
}

/** A button for each day of the week, toggling that day in a signal, as one widget. */
export function WeekdayPicker(props: WeekdayPickerProps) {
  return (
    <DayButtons
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      grow={props.grow}
      columns={props.columns}
      gap={props.gap}
      value={props.value.value}
      onChange={(event) => {
        props.value.value = event.value;
        props.onChange?.({ value: event.value });
      }}
    />
  );
}
