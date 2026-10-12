// Ready-made components kept in the shared library, built from the widgets in widgets.tsx.
// They are written in the same subset as apps, so the compiler can build the library from them.

import type { ComponentChildren } from "preact";
import type { Signal } from "@preact/signals";
import {
  battery_percent,
  current_weather,
  format_number,
  format_temperature,
  format_time,
  heart_rate,
  is_charging,
  is_heart_rate_running,
  is_phone_connected,
  next_alarm,
  now,
  settings,
  steps_today,
  unread_notifications,
  type ChangeEvent,
  type Duration,
  type TimeOfDay,
  HOUR,
  MINUTE,
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
  Stack,
  StatusRow,
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

export interface ListRowProps {
  title: string;
  subtitle?: string;
  icon?: LibraryIcon;
  /** Text on the right, such as a current choice. */
  value?: string;
  /** Children replace the value on the right, such as a `Switch`. */
  children?: ComponentChildren;
  onClick?: () => void;
}

/** A list entry: icon, title, subtitle and a value or control on the right. */
export function ListRow(props: ListRowProps) {
  return (
    <Row gap={8} padding={6} height={props.subtitle ? 56 : 44} onClick={props.onClick}>
      {props.icon && <Icon icon={props.icon} size={28} color="accent" />}
      <Column grow={1}>
        <Label long_mode="scroll">{props.title}</Label>
        {props.subtitle && (
          <Label font="small" color="muted" long_mode="scroll">
            {props.subtitle}
          </Label>
        )}
      </Column>
      {props.value !== undefined && <Label color="muted">{props.value}</Label>}
      {props.children}
    </Row>
  );
}

/** The time, battery and connection, as the system shows them. */
export function StatusBar() {
  return (
    <Row height={24} padding={2} justify="space_between">
      <Label font="small">{format_time(now.value, settings.uses_24_hour_clock.value ? "HH:mm" : "hh:mm")}</Label>
      <Row gap={4}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={18} color="accent" />}
        <BatteryIcon show_percent />
      </Row>
    </Row>
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

/** A question with two buttons, shown over the screen while a signal is true. */
export function ConfirmDialog(props: ConfirmDialogProps) {
  if (!props.open.value) return null;
  return (
    <Stack x={0} y={0} width={240} height={240} background="#000000">
      <Column padding={16} gap={16} width={240} align_items="stretch">
        <Label text_align="center">{props.question}</Label>
        <Row gap={8}>
          <Button
            grow={1}
            onClick={() => {
              props.open.value = false;
              props.onCancel?.();
            }}
          >
            {props.cancel_text ?? "No"}
          </Button>
          <Button
            grow={1}
            background="danger"
            onClick={() => {
              props.open.value = false;
              props.onConfirm();
            }}
          >
            {props.confirm_text ?? "Yes"}
          </Button>
        </Row>
      </Column>
    </Stack>
  );
}

// ============================================================================
// Pickers
// ============================================================================

function number_options(minimum: number, maximum: number, width: number): string[] {
  const options: string[] = [];
  for (let value = minimum; value <= maximum; value++) options.push(format_number(value, width));
  return options;
}

export interface NumberPickerProps extends CommonProps {
  value: Signal<number>;
  minimum: number;
  maximum: number;
  /** Digits to pad to, such as 2 for minutes. */
  digits?: number;
  onChange?: (event: ChangeEvent<number>) => void;
}

/** A roller set up for a whole number. */
export function NumberPicker(props: NumberPickerProps) {
  return (
    <Roller
      {...props}
      options={number_options(props.minimum, props.maximum, props.digits ?? 1)}
      value={props.value.value - props.minimum}
      onChange={(event) => {
        props.value.value = event.value + props.minimum;
        props.onChange?.({ value: props.value.value });
      }}
    />
  );
}

export interface TimePickerProps {
  value: Signal<TimeOfDay>;
  onChange?: (event: ChangeEvent<TimeOfDay>) => void;
}

/** Rollers for an hour and a minute, following the 12 or 24 hour setting. */
export function TimePicker(props: TimePickerProps) {
  const uses_24_hour_clock = settings.uses_24_hour_clock.value;
  const hour = props.value.value.hour;

  function set(next: TimeOfDay) {
    props.value.value = next;
    props.onChange?.({ value: next });
  }

  return (
    <Row gap={6} justify="center">
      <Roller
        options={uses_24_hour_clock ? number_options(0, 23, 2) : number_options(1, 12, 2)}
        value={uses_24_hour_clock ? hour : (hour + 11) % 12}
        onChange={(event) =>
          set({
            ...props.value.value,
            hour: uses_24_hour_clock ? event.value : ((event.value + 1) % 12) + (hour >= 12 ? 12 : 0),
          })
        }
      />
      <Label font="large">:</Label>
      <Roller
        options={number_options(0, 59, 2)}
        value={props.value.value.minute}
        onChange={(event) => set({ ...props.value.value, minute: event.value })}
      />
      {!uses_24_hour_clock && (
        <Button
          width={56}
          onClick={() => set({ ...props.value.value, hour: (hour + 12) % 24 })}
        >
          {hour < 12 ? "AM" : "PM"}
        </Button>
      )}
    </Row>
  );
}

export interface DurationPickerProps {
  value: Signal<Duration>;
  show_hours?: boolean;
  onChange?: (event: ChangeEvent<Duration>) => void;
}

/** Rollers for minutes and seconds, and hours if asked. */
export function DurationPicker(props: DurationPickerProps) {
  const total = props.value.value;
  const hours = Math.floor(total / HOUR);
  const minutes = Math.floor(total / MINUTE) % (props.show_hours ? 60 : 100);
  const seconds = Math.floor(total / SECOND) % 60;

  function set(next_hours: number, next_minutes: number, next_seconds: number) {
    props.value.value = next_hours * HOUR + next_minutes * MINUTE + next_seconds * SECOND;
    props.onChange?.({ value: props.value.value });
  }

  return (
    <Row gap={6} justify="center">
      {props.show_hours && (
        <Roller options={number_options(0, 23, 2)} value={hours} onChange={(event) => set(event.value, minutes, seconds)} />
      )}
      {props.show_hours && <Label font="large">:</Label>}
      <Roller
        options={number_options(0, props.show_hours ? 59 : 99, 2)}
        value={minutes}
        onChange={(event) => set(props.show_hours ? hours : 0, event.value, seconds)}
      />
      <Label font="large">:</Label>
      <Roller
        options={number_options(0, 59, 2)}
        value={seconds}
        onChange={(event) => set(props.show_hours ? hours : 0, minutes, event.value)}
      />
    </Row>
  );
}

export interface MenuItem {
  title: string;
  icon?: LibraryIcon;
  onClick: () => void;
}

/** A list of named actions. */
export function Menu(props: { items: MenuItem[] }) {
  return (
    <Column gap={2}>
      {props.items.map((item) => (
        <ListRow title={item.title} icon={item.icon} onClick={item.onClick} />
      ))}
    </Column>
  );
}
