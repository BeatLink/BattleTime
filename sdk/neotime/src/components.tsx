// Ready-made components kept in the shared library, built from the widgets in widgets.tsx.
// They are written in the same subset as apps, so the compiler can build the library from them.

import type { ComponentChildren } from "preact";
import type { Signal } from "@preact/signals";
import {
  battery_percent,
  format_number,
  format_time,
  is_charging,
  is_phone_connected,
  now,
  settings,
  type ChangeEvent,
  type Duration,
  type TimeOfDay,
  HOUR,
  MINUTE,
  SECOND,
} from "./index.ts";
import { icons, type LibraryIcon } from "./icons.ts";
import { Button, Column, Icon, Label, Roller, Row, Stack, type CommonProps, type FlexProps } from "./widgets.tsx";

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
    <Row height={24} padding={2} justify="between">
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
}

/** The battery's charge and whether it is charging, drawn the way the system does. */
export function BatteryIcon(props: BatteryIconProps) {
  const percent = battery_percent.value;
  const fill_color = is_charging.value ? "success" : percent <= 20 ? "danger" : props.color ?? "text";
  return (
    <Row gap={4}>
      {is_charging.value && <Icon icon={icons.charging} size={16} color="warning" />}
      <Row width={28} height={14} padding={2} border_color={props.color ?? "text"} border_width={2} radius={3}>
        <Column width={Math.round((20 * percent) / 100)} height="100%" background={fill_color} />
      </Row>
      {props.show_percent && <Label font="small">{`${percent}%`}</Label>}
    </Row>
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
