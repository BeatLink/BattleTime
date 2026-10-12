// The widgets an app's screens are built from, one for each rlvgl widget the runtime creates.
// In a browser each is a Preact component styled like rlvgl's dark theme on a 240x240 screen.

import { batch, type Signal } from "@preact/signals";
import type { ComponentChildren, JSX } from "preact";
import { useEffect, useRef } from "preact/hooks";
import type {
  ButtonEvent,
  ChangeEvent,
  ClickEvent,
  Color,
  DragEvent,
  Instant,
  SwipeDirection,
  SwipeEvent,
  ThemeColorName,
} from "./index.ts";
import { icon_symbol, icons, is_icon, type LibraryIcon } from "./icons.ts";
import { installed_apps } from "./index.ts";

// ============================================================================
// Theme
// ============================================================================

/** The theme's colours, which widgets also accept by name, such as `color="accent"`. */
export const theme: Record<ThemeColorName, `#${string}`> = {
  background: "#000000",
  surface: "#383838",
  text: "#ffffff",
  muted: "#a0a0a0",
  accent: "#2f6fdf",
  success: "#20a040",
  warning: "#f0a000",
  danger: "#d02828",
};

export function css_color(color: Color | undefined): string | undefined {
  if (color === undefined) return undefined;
  return color.startsWith("#") ? color : theme[color as ThemeColorName];
}

/** Text sizes, plus `giant` and the display fonts, which hold digits and `:` only. */
export type Font =
  | "small"
  | "normal"
  | "medium"
  | "large"
  | "huge"
  | "giant"
  | "seven_segment"
  | "dot_matrix"
  | "condensed";

const FONT_SIZES_IN_PIXELS: Record<Font, number> = {
  small: 16,
  normal: 20,
  medium: 30,
  large: 42,
  huge: 76,
  giant: 120,
  seven_segment: 76,
  dot_matrix: 40,
  condensed: 48,
};

const FONT_FAMILY = "'JetBrains Mono', 'DejaVu Sans Mono', monospace";

const DISPLAY_FONT_FAMILIES: Partial<Record<Font, string>> = {
  seven_segment: "'DSEG7 Classic', 'JetBrains Mono', monospace",
  dot_matrix: "'DotGothic16', 'JetBrains Mono', monospace",
  condensed: "'Bebas Neue', 'Oswald', 'Arial Narrow', sans-serif",
};

export const SCREEN_SIZE = 240;

// ============================================================================
// Handlers
// ============================================================================

type HandlerFailureListener = (error: unknown) => void;

const handler_failure_listeners: HandlerFailureListener[] = [];

export function listen_for_handler_failures(listener: HandlerFailureListener): () => void {
  handler_failure_listeners.push(listener);
  return () => {
    handler_failure_listeners.splice(handler_failure_listeners.indexOf(listener), 1);
  };
}

/** Runs a handler as one transaction, as the watch does, and reports a failure to the preview. */
export function run_handler<Event>(handler: ((event: Event) => void) | undefined, event: Event) {
  if (!handler) return;
  try {
    batch(() => handler(event));
  } catch (error) {
    for (const listener of handler_failure_listeners) listener(error);
  }
}

type Bindable<Value> = Value | Signal<Value>;

function is_signal<Value>(value: Bindable<Value>): value is Signal<Value> {
  return typeof value === "object" && value !== null && "peek" in value;
}

function read_bound<Value>(value: Bindable<Value>): Value {
  return is_signal(value) ? value.value : value;
}

/** Writes a widget's new value back to the signal it is bound to, then calls `onChange`. */
function change_value<Value>(
  bound: Bindable<Value>,
  on_change: ((event: ChangeEvent<Value>) => void) | undefined,
  value: Value,
) {
  run_handler((event: ChangeEvent<Value>) => {
    if (is_signal(bound)) bound.value = event.value;
    on_change?.(event);
  }, { value });
}

// ============================================================================
// Common props
// ============================================================================

export type Length = number | "100%";

export interface CommonProps {
  x?: number;
  y?: number;
  width?: Length;
  height?: Length;
  /** The share of spare space this widget takes in a `Row` or `Column`. */
  grow?: number;
  padding?: number;
  margin?: number;
  background?: Color;
  color?: Color;
  border_color?: Color;
  border_width?: number;
  radius?: number;
  font?: Font;
  align?: "start" | "center" | "end";
  /** Milliseconds a changed bound property takes to slide to its new value. */
  animate?: number;
  /** 0 is see-through, 255 solid. */
  opacity?: number;
}

function length_css(length: Length): string {
  return length === "100%" ? "100%" : `${length}px`;
}

function common_style(props: CommonProps): JSX.CSSProperties {
  const style: JSX.CSSProperties = { boxSizing: "border-box", flexShrink: 0 };
  if (props.x !== undefined || props.y !== undefined) {
    style.position = "absolute";
    style.left = `${props.x ?? 0}px`;
    style.top = `${props.y ?? 0}px`;
  }
  if (props.width !== undefined) style.width = length_css(props.width);
  if (props.height !== undefined) style.height = length_css(props.height);
  if (props.grow !== undefined) style.flexGrow = props.grow;
  if (props.padding !== undefined) style.padding = `${props.padding}px`;
  if (props.margin !== undefined) style.margin = `${props.margin}px`;
  if (props.background) style.background = css_color(props.background);
  if (props.color) style.color = css_color(props.color);
  if (props.border_color || props.border_width) {
    style.border = `${props.border_width ?? 2}px solid ${css_color(props.border_color) ?? theme.muted}`;
  }
  if (props.radius !== undefined) style.borderRadius = `${props.radius}px`;
  if (props.font) style.fontSize = `${FONT_SIZES_IN_PIXELS[props.font]}px`;
  if (props.font && DISPLAY_FONT_FAMILIES[props.font]) style.fontFamily = DISPLAY_FONT_FAMILIES[props.font];
  if (props.opacity !== undefined) style.opacity = props.opacity / 255;
  if (props.align) style.alignSelf = { start: "flex-start", center: "center", end: "flex-end" }[props.align];
  if (props.animate) style.transition = `all ${props.animate}ms`;
  return style;
}

function css_without_color(props: CommonProps): JSX.CSSProperties {
  return common_style({ ...props, color: undefined });
}

// ============================================================================
// Touch
// ============================================================================

export interface TouchProps {
  onClick?: (event: ClickEvent) => void;
  onLongPress?: (event: ClickEvent) => void;
  onDoubleClick?: (event: ClickEvent) => void;
  onPress?: (event: ClickEvent) => void;
  onRelease?: (event: ClickEvent) => void;
  onDrag?: (event: DragEvent) => void;
}

const LONG_PRESS_IN_MILLISECONDS = 500;

/** Screen coordinates of a pointer event, whatever size the preview draws the screen. */
function screen_point(event: PointerEvent | MouseEvent): ClickEvent {
  const screen = (event.currentTarget as HTMLElement).closest("[data-widget=screen]") as HTMLElement | null;
  const bounds = screen?.getBoundingClientRect();
  if (!bounds || bounds.width === 0) return { x: 0, y: 0 };
  const scale = bounds.width / SCREEN_SIZE;
  return {
    x: Math.round((event.clientX - bounds.left) / scale),
    y: Math.round((event.clientY - bounds.top) / scale),
  };
}

/** Pointer listeners that turn presses into the reference's touch events. */
function use_touch(props: TouchProps, disabled = false) {
  const start = useRef<ClickEvent | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const long_pressed = useRef(false);

  if (disabled) return {};

  return {
    onPointerDown(event: PointerEvent) {
      const point = screen_point(event);
      start.current = point;
      long_pressed.current = false;
      run_handler(props.onPress, point);
      if (props.onLongPress) {
        timer.current = setTimeout(() => {
          long_pressed.current = true;
          run_handler(props.onLongPress, point);
        }, LONG_PRESS_IN_MILLISECONDS);
      }
      if (props.onDrag) {
        (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
      }
    },
    onPointerMove(event: PointerEvent) {
      if (!start.current || !props.onDrag) return;
      const point = screen_point(event);
      run_handler(props.onDrag, { ...point, start_x: start.current.x, start_y: start.current.y });
    },
    onPointerUp(event: PointerEvent) {
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
      if (start.current) run_handler(props.onRelease, screen_point(event));
      start.current = null;
    },
    onClick(event: MouseEvent) {
      if (long_pressed.current) return;
      if (!props.onClick) return;
      event.stopPropagation();
      run_handler(props.onClick, screen_point(event));
    },
    onDblClick(event: MouseEvent) {
      if (!props.onDoubleClick) return;
      event.stopPropagation();
      run_handler(props.onDoubleClick, screen_point(event));
    },
  };
}

// ============================================================================
// Screen
// ============================================================================

export interface ScreenHandlers {
  onButton?: (event: ButtonEvent) => void;
  onSwipe?: (event: SwipeEvent) => void;
}

/** The handlers of the screen on show, which the preview calls for the button and swipes. */
export const active_screen: ScreenHandlers = {};

const SWIPE_DISTANCE_IN_PIXELS = 40;

export interface ScreenProps extends ScreenHandlers, TouchProps {
  children?: ComponentChildren;
  background?: Color;
  padding?: number;
  gap?: number;
  /** Lays the children out across instead of down. */
  direction?: "column" | "row";
  align_items?: "start" | "center" | "end" | "stretch";
  justify?: Justification;
  onShow?: () => void;
  onHide?: () => void;
}

/** The root of every screen. Children flow down it like a `Column` unless they have `x` or `y`. */
export function Screen(props: ScreenProps) {
  const swipe_start = useRef<ClickEvent | null>(null);
  const touch = use_touch(props);
  active_screen.onButton = props.onButton;
  active_screen.onSwipe = props.onSwipe;

  useEffect(() => {
    run_handler(props.onShow, undefined);
    return () => {
      active_screen.onButton = undefined;
      active_screen.onSwipe = undefined;
      run_handler(props.onHide, undefined);
    };
  }, []);

  function pointer_down(event: PointerEvent) {
    swipe_start.current = screen_point(event);
    touch.onPointerDown?.(event);
  }

  function pointer_up(event: PointerEvent) {
    touch.onPointerUp?.(event);
    if (!swipe_start.current) return;
    const end = screen_point(event);
    const across = end.x - swipe_start.current.x;
    const down = end.y - swipe_start.current.y;
    swipe_start.current = null;
    if (Math.max(Math.abs(across), Math.abs(down)) < SWIPE_DISTANCE_IN_PIXELS) return;
    const direction: SwipeDirection =
      Math.abs(across) > Math.abs(down) ? (across > 0 ? "right" : "left") : down > 0 ? "down" : "up";
    swipe(direction);
  }

  return (
    <div
      data-widget="screen"
      {...touch}
      onPointerDown={pointer_down}
      onPointerUp={pointer_up}
      style={{
        position: "relative",
        width: `${SCREEN_SIZE}px`,
        height: `${SCREEN_SIZE}px`,
        overflow: "hidden",
        background: css_color(props.background) ?? theme.background,
        color: theme.text,
        fontFamily: FONT_FAMILY,
        fontSize: `${FONT_SIZES_IN_PIXELS.normal}px`,
        display: "flex",
        flexDirection: props.direction ?? "column",
        boxSizing: "border-box",
        padding: props.padding !== undefined ? `${props.padding}px` : undefined,
        gap: props.gap !== undefined ? `${props.gap}px` : undefined,
        alignItems: items_css(props.align_items ?? "stretch"),
        justifyContent: justification_css(props.justify ?? "start"),
        userSelect: "none",
        touchAction: "none",
      }}
    >
      {props.children}
    </div>
  );
}

/** Gives a swipe to the screen on show, or to the preview's system handling if it has none. */
export const swipe_fallback: { handle: (direction: SwipeDirection) => void } = { handle: () => {} };

export function swipe(direction: SwipeDirection) {
  if (active_screen.onSwipe) run_handler(active_screen.onSwipe, { direction });
  else swipe_fallback.handle(direction);
}

// ============================================================================
// Layout
// ============================================================================

export type Justification = "start" | "center" | "end" | "space_between";

function items_css(alignment: "start" | "center" | "end" | "stretch"): string {
  return { start: "flex-start", center: "center", end: "flex-end", stretch: "stretch" }[alignment];
}

function justification_css(justification: Justification): string {
  return {
    start: "flex-start",
    center: "center",
    end: "flex-end",
    space_between: "space-between",
  }[justification];
}

export interface FlexProps extends CommonProps, TouchProps {
  children?: ComponentChildren;
  gap?: number;
  /** How children line up across the direction of flow. */
  align_items?: "start" | "center" | "end" | "stretch";
  justify?: Justification;
}

function Flex(props: FlexProps & { direction: "row" | "column"; widget: string }) {
  const touch = use_touch(props);
  return (
    <div
      data-widget={props.widget}
      {...touch}
      style={{
        position: "relative",
        display: "flex",
        flexDirection: props.direction,
        gap: props.gap !== undefined ? `${props.gap}px` : undefined,
        alignItems: items_css(props.align_items ?? (props.direction === "row" ? "center" : "stretch")),
        justifyContent: justification_css(props.justify ?? "start"),
        ...common_style(props),
      }}
    >
      {props.children}
    </div>
  );
}

/** Children one after another, down. */
export function Column(props: FlexProps) {
  return <Flex {...props} direction="column" widget="column" />;
}

/** Children one after another, across. */
export function Row(props: FlexProps) {
  return <Flex {...props} direction="row" widget="row" />;
}

export interface GridProps extends CommonProps {
  children?: ComponentChildren;
  columns: number;
  gap?: number;
}

/** Children in rows of equal cells, `columns` to a row. */
export function Grid(props: GridProps) {
  return (
    <div
      data-widget="grid"
      style={{
        position: "relative",
        display: "grid",
        gridTemplateColumns: `repeat(${props.columns}, 1fr)`,
        gridAutoRows: "1fr",
        gap: props.gap !== undefined ? `${props.gap}px` : undefined,
        ...common_style(props),
      }}
    >
      {props.children}
    </div>
  );
}

export interface StackProps extends CommonProps, TouchProps {
  children?: ComponentChildren;
}

/** Children on top of each other, placed by their `x` and `y` or centred. */
export function Stack(props: StackProps) {
  const touch = use_touch(props);
  return (
    <div
      data-widget="stack"
      {...touch}
      style={{
        position: "relative",
        display: "grid",
        placeItems: "center",
        ...common_style(props),
      }}
    >
      {props.children}
    </div>
  );
}

/** Content taller than its space, scrolled by dragging. */
export function Scroll(props: FlexProps) {
  return (
    <div
      data-widget="scroll"
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        gap: props.gap !== undefined ? `${props.gap}px` : undefined,
        overflowY: "auto",
        minHeight: 0,
        ...common_style(props),
      }}
    >
      {props.children}
    </div>
  );
}

/** A scrolling list of rows, drawn in the browser like a `Scroll`. */
export function List(props: FlexProps) {
  return (
    <div
      data-widget="list"
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        gap: props.gap !== undefined ? `${props.gap}px` : undefined,
        overflowY: "auto",
        minHeight: 0,
        ...common_style(props),
      }}
    >
      {props.children}
    </div>
  );
}

export interface AppListProps extends CommonProps {
  /** Pixels between rows. */
  gap?: number;
  /** Receives the place in `installed_apps` of the app whose row is tapped. */
  onClick?: (event: ChangeEvent<number>) => void;
}

/** The installed apps as a scrolling list of rows, each its icon and name, drawn by the watch as one widget. */
export function AppList(props: AppListProps) {
  return (
    <List padding={props.padding} gap={props.gap} {...common_props_only(props)}>
      {installed_apps.value.map((app, place) => (
        <Button height={52} onClick={() => props.onClick && run_handler(props.onClick, { value: place })}>
          <Row gap={12} align_items="center" padding={8}>
            <Icon icon={app.icon} size={32} />
            <Label grow={1} long_mode="clip">
              {app.name}
            </Label>
          </Row>
        </Button>
      ))}
    </List>
  );
}

/** The props every widget shares, without the ones `AppList` handles itself. */
function common_props_only(props: AppListProps): CommonProps {
  const { gap: _gap, onClick: _on_click, padding: _padding, ...common } = props;
  return common;
}

// ============================================================================
// Showing things
// ============================================================================

export interface LabelProps extends CommonProps, TouchProps {
  children?: ComponentChildren;
  text_align?: "left" | "center" | "right";
  /** Long text wraps by default; `scroll` keeps it on one line and scrolls it. */
  long_mode?: "wrap" | "scroll" | "clip";
}

export function Label(props: LabelProps) {
  const touch = use_touch(props);
  const long_mode = props.long_mode ?? "wrap";
  return (
    <div
      data-widget="label"
      {...touch}
      style={{
        lineHeight: 1.15,
        textAlign: props.text_align ?? "left",
        whiteSpace: long_mode === "wrap" ? "pre-wrap" : "pre",
        overflow: "hidden",
        textOverflow: long_mode === "scroll" ? "ellipsis" : undefined,
        ...common_style(props),
      }}
    >
      {props.children}
    </div>
  );
}

export interface IconProps extends CommonProps, TouchProps {
  icon: LibraryIcon;
  /** The size the icon is drawn at, in pixels. */
  size?: number;
}

/** A shared library icon, drawn in the `color` given. */
export function Icon(props: IconProps) {
  const size = props.size ?? 32;
  const touch = use_touch(props);
  return (
    <div
      data-widget="icon"
      {...touch}
      style={{
        fontSize: `${Math.round(size * 0.8)}px`,
        lineHeight: `${size}px`,
        minWidth: `${size}px`,
        height: `${size}px`,
        textAlign: "center",
        ...common_style(props),
      }}
    >
      {icon_symbol(props.icon)}
    </div>
  );
}

// ============================================================================
// Battery, status icons and badges, each drawn by the watch as one widget
// ============================================================================

export interface BatteryGaugeProps extends CommonProps {
  /** The charge in percent. */
  value: number;
  charging?: boolean;
  /** Draws it all in `color`, rather than green while charging and red when low. */
  single_color?: boolean;
  /** Whether the charging bolt shows while charging; it does unless this is false. */
  show_bolt?: boolean;
  show_percent?: boolean;
}

/** A battery's outline filled to its charge, with a bolt while charging. Use `BatteryIcon` for the watch's own. */
export function BatteryGauge(props: BatteryGaugeProps) {
  const own_color = props.color ?? "text";
  const fill_color = props.single_color
    ? own_color
    : props.charging
      ? "success"
      : props.value <= 20
        ? "danger"
        : own_color;
  return (
    <div data-widget="battery-gauge" style={{ display: "flex", gap: "4px", alignItems: "flex-start", ...common_style(props) }}>
      {props.charging && props.show_bolt !== false && (
        <Icon icon={icons.charging} size={16} color={props.single_color ? own_color : "warning"} />
      )}
      <div
        data-widget="battery-body"
        style={{
          boxSizing: "border-box",
          width: "28px",
          height: "14px",
          padding: "2px",
          border: `2px solid ${css_color(own_color)}`,
          borderRadius: "3px",
          flexShrink: 0,
        }}
      >
        <div style={{ width: `${Math.round((20 * props.value) / 100)}px`, height: "100%", background: css_color(fill_color) }} />
      </div>
      {props.show_percent && <span style={{ fontSize: `${FONT_SIZES_IN_PIXELS.small}px` }}>{`${props.value}%`}</span>}
    </div>
  );
}

export interface StatusRowProps extends BatteryGaugeProps {
  /** Which icons show before the battery: 1 notifications, 2 the phone, 4 an alarm. */
  status_flags?: number;
  icon_size?: number;
  gap?: number;
}

/** Status icons followed by a battery, drawn as one widget. Use `StatusIcons` for the watch's own. */
export function StatusRow(props: StatusRowProps) {
  const flags = props.status_flags ?? 0;
  const size = props.icon_size ?? 22;
  const icons_shown = [
    flags & 1 ? icons.notification : null,
    flags & 2 ? icons.bluetooth : null,
    flags & 4 ? icons.alarm : null,
  ].filter((icon) => icon !== null);
  const justify = props.align === "end" ? "flex-end" : props.align === "center" ? "center" : "flex-start";
  return (
    <div
      data-widget="status-row"
      style={{ display: "flex", gap: `${props.gap ?? 0}px`, alignItems: "flex-start", justifyContent: justify, ...common_style(props) }}
    >
      {icons_shown.map((icon) => (
        <Icon icon={icon} size={size} color={props.color} />
      ))}
      <BatteryGauge
        value={props.value}
        charging={props.charging}
        single_color={props.single_color}
        show_bolt={props.show_bolt}
        show_percent={props.show_percent}
        color={props.border_color}
      />
    </div>
  );
}

export interface BadgeProps extends CommonProps {
  icon: LibraryIcon;
  children?: ComponentChildren;
  icon_size?: number;
  gap?: number;
  /** The icon's colour, when it is not the text's. */
  recolor?: Color;
}

/** An icon and a text beside it, drawn as one widget. */
export function Badge(props: BadgeProps) {
  const justify = props.align === "end" ? "flex-end" : props.align === "center" ? "center" : "flex-start";
  return (
    <div data-widget="badge" style={{ display: "flex", gap: `${props.gap ?? 0}px`, justifyContent: justify, alignItems: "flex-start", ...common_style(props) }}>
      <Icon icon={props.icon} size={props.icon_size ?? 20} color={props.recolor ?? props.color} />
      <span data-widget="badge-text" style={{ alignSelf: "center" }}>
        {props.children}
      </span>
    </div>
  );
}

export interface ImageProps extends CommonProps {
  /** A file next to the app's source, or a shared library image. */
  src: string | LibraryIcon;
  /** Draws a one-colour image, such as a logo, in this colour. */
  recolor?: Color;
  size?: number;
}

/** A picture from the app or the shared library. */
export function Image(props: ImageProps) {
  if (is_icon(props.src)) return <Icon {...props} icon={props.src} color={props.recolor ?? props.color} />;
  return <img data-widget="image" src={props.src} alt="" style={common_style(props)} />;
}

export interface LineProps extends CommonProps {
  /** Points in the parent's pixels, joined in order. */
  points: { x: number; y: number }[];
  thickness?: number;
  rounded?: boolean;
}

/** Lines between points, such as clock ticks. */
export function Line(props: LineProps) {
  return (
    <svg
      data-widget="line"
      width={SCREEN_SIZE}
      height={SCREEN_SIZE}
      style={{ position: "absolute", left: 0, top: 0, pointerEvents: "none", overflow: "visible" }}
    >
      <polyline
        points={props.points.map((point) => `${point.x},${point.y}`).join(" ")}
        stroke={css_color(props.color) ?? theme.text}
        stroke-width={props.thickness ?? 2}
        stroke-linecap={props.rounded === false ? "butt" : "round"}
        fill="none"
      />
    </svg>
  );
}

export interface ArcProps extends CommonProps, TouchProps {
  /** Given a signal, the arc can be dragged and writes the new value back to it. */
  value: Bindable<number>;
  onChange?: (event: ChangeEvent<number>) => void;
  minimum?: number;
  maximum?: number;
  /** Degrees clockwise from twelve o'clock. */
  start_angle?: number;
  end_angle?: number;
  thickness?: number;
  track_color?: Color;
  diameter?: number;
  children?: ComponentChildren;
}

function arc_path(center: number, radius: number, start_degrees: number, sweep_degrees: number): string {
  const point = (degrees: number) => {
    const radians = (degrees * Math.PI) / 180;
    return `${center + Math.sin(radians) * radius} ${center - Math.cos(radians) * radius}`;
  };
  const sweep = Math.min(sweep_degrees, 359.99);
  return `M ${point(start_degrees)} A ${radius} ${radius} 0 ${sweep > 180 ? 1 : 0} 1 ${point(start_degrees + sweep)}`;
}

/** A ring, or part of one, that fills clockwise with its value; children sit in the middle. */
export function Arc(props: ArcProps) {
  const diameter = props.diameter ?? 200;
  const thickness = props.thickness ?? 12;
  const minimum = props.minimum ?? 0;
  const maximum = props.maximum ?? 100;
  const start = props.start_angle ?? 225;
  const end = props.end_angle ?? 135;
  const span = (((end - start) % 360) + 360) % 360 || 360;
  const share = Math.min(1, Math.max(0, (read_bound(props.value) - minimum) / (maximum - minimum || 1)));
  const radius = (diameter - thickness) / 2;
  const center = diameter / 2;
  const touch = use_touch(
    is_signal(props.value)
      ? {
          ...props,
          onDrag: (event) => {
            const bounds_left = typeof props.x === "number" ? props.x : 0;
            const bounds_top = typeof props.y === "number" ? props.y : 0;
            const across = event.x - bounds_left - center;
            const down = event.y - bounds_top - center;
            const degrees = (((Math.atan2(across, -down) * 180) / Math.PI - start) % 360 + 360) % 360;
            if (degrees > span) return;
            const value = Math.round(minimum + ((maximum - minimum) * degrees) / span);
            change_value(props.value, props.onChange, value);
          },
        }
      : props,
  );
  return (
    <div
      data-widget="arc"
      {...touch}
      style={{ position: "relative", width: `${diameter}px`, height: `${diameter}px`, ...css_without_color(props) }}
    >
      <svg width={diameter} height={diameter} style={{ position: "absolute", left: 0, top: 0 }}>
        <path
          d={arc_path(center, radius, start, span)}
          stroke={css_color(props.track_color) ?? theme.surface}
          stroke-width={thickness}
          fill="none"
          stroke-linecap="round"
        />
        {share > 0 && (
          <path
            d={arc_path(center, radius, start, span * share)}
            stroke={css_color(props.color) ?? theme.accent}
            stroke-width={thickness}
            fill="none"
            stroke-linecap="round"
          />
        )}
      </svg>
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {props.children}
      </div>
    </div>
  );
}

export interface BarProps extends CommonProps {
  value: number;
  minimum?: number;
  maximum?: number;
  track_color?: Color;
}

/** A straight bar that fills from the left. */
export function Bar(props: BarProps) {
  const minimum = props.minimum ?? 0;
  const maximum = props.maximum ?? 100;
  const share = Math.min(1, Math.max(0, (props.value - minimum) / (maximum - minimum || 1)));
  return (
    <div
      data-widget="bar"
      style={{
        height: "8px",
        borderRadius: "4px",
        overflow: "hidden",
        background: css_color(props.track_color) ?? theme.surface,
        ...css_without_color(props),
      }}
    >
      <div style={{ width: `${share * 100}%`, height: "100%", background: css_color(props.color) ?? theme.accent }} />
    </div>
  );
}

export interface LedProps extends CommonProps {
  /** 0 is off, 255 fully on. */
  brightness?: number;
  diameter?: number;
}

/** A dot that is on, off or dimmed. */
export function Led(props: LedProps) {
  const diameter = props.diameter ?? 16;
  return (
    <div
      data-widget="led"
      style={{
        width: `${diameter}px`,
        height: `${diameter}px`,
        borderRadius: "50%",
        background: css_color(props.color) ?? theme.accent,
        opacity: 0.15 + 0.85 * ((props.brightness ?? 255) / 255),
        ...css_without_color(props),
      }}
    />
  );
}

export interface SpinnerProps extends CommonProps {
  diameter?: number;
}

/** Shows that something is in progress. */
export function Spinner(props: SpinnerProps) {
  const diameter = props.diameter ?? 40;
  return (
    <div
      data-widget="spinner"
      style={{
        width: `${diameter}px`,
        height: `${diameter}px`,
        borderRadius: "50%",
        border: `4px solid ${theme.surface}`,
        borderTopColor: css_color(props.color) ?? theme.accent,
        animation: "neotime-spin 1s linear infinite",
        ...css_without_color(props),
      }}
    />
  );
}

export interface ChartSeries {
  values: number[];
  color?: Color;
}

export interface ChartProps extends CommonProps {
  /** One series, drawn in `color`. */
  values?: number[];
  /** Up to 3 series, each with its colour, in place of `values`. */
  series?: ChartSeries[];
  minimum?: number;
  maximum?: number;
  kind?: "line" | "bars";
}

/** A line or bar chart of one to three fixed-length arrays. */
export function Chart(props: ChartProps) {
  const width = typeof props.width === "number" ? props.width : 220;
  const height = typeof props.height === "number" ? props.height : 100;
  const series = (props.series ?? [{ values: props.values ?? [], color: props.color }]).slice(0, 3);
  const every_value = series.flatMap((one) => one.values);
  const minimum = props.minimum ?? Math.min(0, ...every_value);
  const maximum = props.maximum ?? Math.max(1, ...every_value);
  const span = maximum - minimum || 1;
  const row_of = (value: number) => height - ((Math.min(maximum, Math.max(minimum, value)) - minimum) / span) * height;
  return (
    <svg data-widget="chart" width={width} height={height} style={css_without_color(props)}>
      {series.map((one, series_index) => {
        const color = css_color(one.color) ?? theme.accent;
        const slot_width = width / Math.max(1, one.values.length);
        const bar_width = slot_width / series.length;
        const step = one.values.length > 1 ? width / (one.values.length - 1) : width;
        return props.kind === "bars" ? (
          one.values.map((value, index) => (
            <rect
              x={index * slot_width + series_index * bar_width + 1}
              y={row_of(value)}
              width={Math.max(1, bar_width - 2)}
              height={height - row_of(value)}
              fill={color}
            />
          ))
        ) : (
          <polyline
            points={one.values.map((value, index) => `${index * step},${row_of(value)}`).join(" ")}
            stroke={color}
            stroke-width={2}
            fill="none"
          />
        );
      })}
    </svg>
  );
}

export interface ScaleProps extends CommonProps {
  /** Ticks evenly spaced around a full circle. */
  ticks: number;
  /** Every this many ticks is a longer, thicker one. */
  major_every?: number;
  diameter?: number;
  /** Labels for the major ticks, starting at twelve o'clock. */
  labels?: string[];
  major_color?: Color;
  /** Tick numbers to leave out, such as under a date window. */
  skip?: number[];
  /** Where the ticks start and end, in degrees clockwise from twelve o'clock; a full circle by default. */
  start_angle?: number;
  end_angle?: number;
  /** Where a needle points, counted in ticks from twelve o'clock, for a gauge. */
  value?: number;
  needle_color?: Color;
}

/** Ticks and labels around a dial. */
export function Scale(props: ScaleProps) {
  const diameter = props.diameter ?? SCREEN_SIZE;
  const center = diameter / 2;
  const major_every = props.major_every ?? 1;
  const start_angle = props.start_angle ?? 0;
  const sweep = props.end_angle === undefined ? 360 : (((props.end_angle - start_angle) % 360) + 360) % 360 || 360;
  const tick_spacing = sweep === 360 ? sweep / props.ticks : sweep / Math.max(1, props.ticks - 1);
  const radians_of = (tick: number) => ((start_angle + tick * tick_spacing) * Math.PI) / 180;
  const marks = [];
  for (let tick = 0; tick < props.ticks; tick++) {
    if (props.skip?.includes(tick)) continue;
    const is_major = tick % major_every === 0;
    const radians = radians_of(tick);
    const outer = center - 2;
    const inner = outer - (is_major ? 14 : 7);
    marks.push(
      <line
        x1={center + Math.sin(radians) * inner}
        y1={center - Math.cos(radians) * inner}
        x2={center + Math.sin(radians) * outer}
        y2={center - Math.cos(radians) * outer}
        stroke={css_color(is_major ? props.major_color ?? props.color : props.color) ?? theme.muted}
        stroke-width={is_major ? 4 : 2}
      />,
    );
    if (is_major && props.labels) {
      const label = props.labels[tick / major_every];
      const label_radius = inner - 16;
      marks.push(
        <text
          x={center + Math.sin(radians) * label_radius}
          y={center - Math.cos(radians) * label_radius}
          fill={theme.text}
          font-size="18"
          font-family={FONT_FAMILY}
          text-anchor="middle"
          dominant-baseline="central"
        >
          {label}
        </text>,
      );
    }
  }
  return (
    <svg
      data-widget="scale"
      width={diameter}
      height={diameter}
      style={{ position: "absolute", left: 0, top: 0, pointerEvents: "none", ...css_without_color(props) }}
    >
      {marks}
      {props.value !== undefined && (
        <line
          x1={center}
          y1={center}
          x2={center + Math.sin(radians_of(props.value)) * center * 0.8}
          y2={center - Math.cos(radians_of(props.value)) * center * 0.8}
          stroke={css_color(props.needle_color) ?? theme.danger}
          stroke-width={3}
          stroke-linecap="round"
        />
      )}
    </svg>
  );
}

export interface AnalogClockProps extends CommonProps {
  time: Instant;
  diameter?: number;
  show_seconds?: boolean;
  hour_color?: Color;
  minute_color?: Color;
  second_color?: Color;
  /** Hand lengths in pixels from the centre. */
  hour_length?: number;
  minute_length?: number;
  second_length?: number;
  hand_style?: "line" | "tapered" | "rounded";
}

/** Clock hands bound to a time, normally `now`. */
export function AnalogClock(props: AnalogClockProps) {
  const diameter = props.diameter ?? SCREEN_SIZE;
  const center = diameter / 2;
  const date = new Date(props.time);
  const seconds = date.getSeconds();
  const minutes = date.getMinutes() + seconds / 60;
  const hours = (date.getHours() % 12) + minutes / 60;

  const hand_style = props.hand_style ?? "rounded";

  function hand(turns: number, length: number, thickness: number, color: string) {
    const radians = turns * 2 * Math.PI;
    const tip_x = center + Math.sin(radians) * length;
    const tip_y = center - Math.cos(radians) * length;
    if (hand_style === "tapered") {
      const side_x = Math.cos(radians) * thickness;
      const side_y = Math.sin(radians) * thickness;
      return (
        <polygon
          points={`${center + side_x},${center + side_y} ${tip_x},${tip_y} ${center - side_x},${center - side_y}`}
          fill={color}
        />
      );
    }
    return (
      <line
        x1={center - Math.sin(radians) * 12}
        y1={center + Math.cos(radians) * 12}
        x2={tip_x}
        y2={tip_y}
        stroke={color}
        stroke-width={thickness}
        stroke-linecap={hand_style === "line" ? "butt" : "round"}
      />
    );
  }

  return (
    <svg
      data-widget="analog-clock"
      width={diameter}
      height={diameter}
      style={{ position: "absolute", left: 0, top: 0, pointerEvents: "none", ...css_without_color(props) }}
    >
      {hand(hours / 12, props.hour_length ?? diameter * 0.27, 7, css_color(props.hour_color) ?? theme.text)}
      {hand(minutes / 60, props.minute_length ?? diameter * 0.4, 5, css_color(props.minute_color) ?? theme.text)}
      {props.show_seconds &&
        hand(seconds / 60, props.second_length ?? diameter * 0.43, 2, css_color(props.second_color) ?? theme.danger)}
      <circle cx={center} cy={center} r={6} fill={css_color(props.second_color) ?? theme.danger} />
    </svg>
  );
}

export interface CellGridProps extends CommonProps {
  columns: number;
  rows: number;
  /** The side of one square cell, in pixels. */
  cell_size: number;
  /** One number per cell, read as a colour from `palette`. */
  cells: number[];
  palette: Color[];
  /** Pixels left empty between cells. */
  cell_gap?: number;
  cell_shape?: "square" | "rounded" | "round";
  /** Receives the number of the cell touched. */
  onClick?: (event: ChangeEvent<number>) => void;
  /** Receives the number of each cell a finger moves over. */
  onDrag?: (event: ChangeEvent<number>) => void;
}

/** A board of coloured cells from one array, drawn as a single widget. */
export function CellGrid(props: CellGridProps) {
  const gap = props.cell_gap ?? 0;
  const corner = props.cell_shape === "round" ? props.cell_size / 2 : props.cell_shape === "rounded" ? props.cell_size / 5 : 0;
  const squares = [];
  for (let cell = 0; cell < props.columns * props.rows; cell++) {
    const color = css_color(props.palette[props.cells[cell] ?? 0]) ?? theme.background;
    squares.push(
      <rect
        x={(cell % props.columns) * props.cell_size + gap / 2}
        y={Math.floor(cell / props.columns) * props.cell_size + gap / 2}
        width={props.cell_size - gap}
        height={props.cell_size - gap}
        rx={corner}
        fill={color}
      />,
    );
  }

  function cell_at(event: MouseEvent): number {
    const bounds = (event.currentTarget as SVGElement).getBoundingClientRect();
    const scale = bounds.width / (props.columns * props.cell_size) || 1;
    const column = Math.floor((event.clientX - bounds.left) / scale / props.cell_size);
    const row = Math.floor((event.clientY - bounds.top) / scale / props.cell_size);
    return row * props.columns + column;
  }

  return (
    <svg
      data-widget="cell-grid"
      width={props.columns * props.cell_size}
      height={props.rows * props.cell_size}
      onClick={(event) => props.onClick && run_handler(props.onClick, { value: cell_at(event) })}
      onPointerMove={(event) => props.onDrag && event.buttons && run_handler(props.onDrag, { value: cell_at(event) })}
      style={{ touchAction: "none", ...css_without_color(props) }}
    >
      {squares}
    </svg>
  );
}

// ============================================================================
// Input
// ============================================================================

export interface ButtonProps extends CommonProps, TouchProps {
  children?: ComponentChildren;
  disabled?: boolean;
}

/** A tappable button with text or an icon. */
export function Button(props: ButtonProps) {
  const touch = use_touch(props, props.disabled);
  return (
    <button
      data-widget="button"
      disabled={props.disabled}
      {...touch}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: "6px",
        minHeight: "40px",
        border: "none",
        borderRadius: "8px",
        fontFamily: FONT_FAMILY,
        fontSize: `${FONT_SIZES_IN_PIXELS.normal}px`,
        color: theme.text,
        background: theme.surface,
        opacity: props.disabled ? 0.45 : 1,
        cursor: props.disabled ? "default" : "pointer",
        padding: "0 8px",
        ...common_style(props),
      }}
    >
      {props.children}
    </button>
  );
}

export interface ClickAreaProps extends CommonProps, TouchProps {
  children?: ComponentChildren;
}

/** An invisible area that takes touches. */
export function ClickArea(props: ClickAreaProps) {
  const touch = use_touch(props);
  return (
    <div data-widget="click-area" {...touch} style={{ touchAction: "none", ...common_style(props) }}>
      {props.children}
    </div>
  );
}

export interface SliderProps extends CommonProps {
  value: Bindable<number>;
  minimum?: number;
  maximum?: number;
  step?: number;
  onChange?: (event: ChangeEvent<number>) => void;
}

export function Slider(props: SliderProps) {
  return (
    <input
      data-widget="slider"
      type="range"
      min={props.minimum ?? 0}
      max={props.maximum ?? 100}
      step={props.step ?? 1}
      value={read_bound(props.value)}
      onInput={(event) =>
        change_value(props.value, props.onChange, Number((event.currentTarget as HTMLInputElement).value))
      }
      style={{ accentColor: css_color(props.color) ?? theme.accent, ...css_without_color(props) }}
    />
  );
}

export interface SwitchProps extends CommonProps {
  value: Bindable<boolean>;
  onChange?: (event: ChangeEvent<boolean>) => void;
}

export function Switch(props: SwitchProps) {
  const on = read_bound(props.value);
  return (
    <div
      data-widget="switch"
      role="switch"
      aria-checked={on}
      onClick={() => change_value(props.value, props.onChange, !on)}
      style={{
        width: "56px",
        height: "30px",
        borderRadius: "15px",
        background: on ? css_color(props.color) ?? theme.accent : theme.surface,
        position: "relative",
        cursor: "pointer",
        ...css_without_color(props),
      }}
    >
      <div
        style={{
          position: "absolute",
          top: "3px",
          left: on ? "29px" : "3px",
          width: "24px",
          height: "24px",
          borderRadius: "12px",
          background: theme.text,
        }}
      />
    </div>
  );
}

export interface CheckboxProps extends CommonProps {
  value: Bindable<boolean>;
  onChange?: (event: ChangeEvent<boolean>) => void;
  children?: ComponentChildren;
}

export function Checkbox(props: CheckboxProps) {
  const ticked = read_bound(props.value);
  return (
    <div
      data-widget="checkbox"
      role="checkbox"
      aria-checked={ticked}
      onClick={() => change_value(props.value, props.onChange, !ticked)}
      style={{ display: "flex", alignItems: "center", gap: "8px", cursor: "pointer", ...common_style(props) }}
    >
      <div
        style={{
          width: "24px",
          height: "24px",
          borderRadius: "4px",
          border: `2px solid ${theme.accent}`,
          background: ticked ? theme.accent : "transparent",
          textAlign: "center",
          lineHeight: "20px",
          fontSize: "18px",
          boxSizing: "border-box",
        }}
      >
        {ticked ? "✓" : ""}
      </div>
      <span>{props.children}</span>
    </div>
  );
}

export interface RollerProps extends CommonProps {
  options: string[];
  /** The index of the chosen option. */
  value: Bindable<number>;
  onChange?: (event: ChangeEvent<number>) => void;
  /** How many options show at once; the chosen one is in the middle. */
  visible_rows?: number;
}

/** A scrolling wheel of choices, turned by dragging or the mouse wheel. */
export function Roller(props: RollerProps) {
  const visible_rows = props.visible_rows ?? 3;
  const font_size = FONT_SIZES_IN_PIXELS[props.font ?? "large"];
  const row_height = Math.round(font_size * 1.25);
  const drag_start = useRef<number | null>(null);
  const half = Math.floor(visible_rows / 2);
  const count = props.options.length;
  const selected = read_bound(props.value);

  function choose(index: number) {
    change_value(props.value, props.onChange, ((index % count) + count) % count);
  }

  const rows = [];
  for (let offset = -half; offset <= half; offset++) {
    const index = (((selected + offset) % count) + count) % count;
    rows.push(
      <div
        data-roller-offset={offset}
        onClick={() => offset !== 0 && choose(selected + offset)}
        style={{
          height: `${row_height}px`,
          lineHeight: `${row_height}px`,
          textAlign: "center",
          color: offset === 0 ? theme.text : theme.muted,
          background: offset === 0 ? theme.surface : "transparent",
          borderRadius: "6px",
        }}
      >
        {props.options[index]}
      </div>,
    );
  }

  return (
    <div
      data-widget="roller"
      onPointerDown={(event) => {
        drag_start.current = event.clientY;
      }}
      onPointerUp={(event) => {
        if (drag_start.current === null) return;
        const moved_rows = Math.round((drag_start.current - event.clientY) / row_height);
        drag_start.current = null;
        if (moved_rows !== 0) choose(selected + moved_rows);
      }}
      onWheel={(event) => {
        event.preventDefault();
        choose(selected + Math.sign(event.deltaY));
      }}
      style={{
        fontSize: `${font_size}px`,
        minWidth: "64px",
        touchAction: "none",
        cursor: "ns-resize",
        ...common_style(props),
      }}
    >
      {rows}
    </div>
  );
}

export interface ButtonMatrixProps extends CommonProps {
  /** One list of labels per row; a click reports the label's place counted across all rows. */
  rows: string[][];
  onClick: (event: ChangeEvent<number>) => void;
  /** Labels whose buttons are drawn in the accent colour. */
  highlighted?: string[];
}

/** A grid of buttons drawn as one widget, such as a keypad. */
export function ButtonMatrix(props: ButtonMatrixProps) {
  let index = 0;
  return (
    <div
      data-widget="button-matrix"
      style={{ display: "flex", flexDirection: "column", gap: "4px", ...common_style(props) }}
    >
      {props.rows.map((row) => (
        <div style={{ display: "flex", gap: "4px", flex: 1 }}>
          {row.map((label) => {
            const button_index = index++;
            return (
              <button
                data-widget="button"
                onClick={() => run_handler(props.onClick, { value: button_index })}
                style={{
                  flex: 1,
                  border: "none",
                  borderRadius: "6px",
                  fontFamily: FONT_FAMILY,
                  fontSize: `${FONT_SIZES_IN_PIXELS[props.font ?? "normal"]}px`,
                  color: theme.text,
                  background: props.highlighted?.includes(label) ? theme.accent : theme.surface,
                  cursor: "pointer",
                  padding: 0,
                }}
              >
                {label}
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}

// ============================================================================
// Pickers and Confirm, each drawn by the watch as one widget
// ============================================================================

function two_digit_options(first: number, count: number): string[] {
  return Array.from({ length: count }, (_, place) => `${first + place}`.padStart(2, "0"));
}

export interface TimeRollersProps extends CommonProps {
  /** The minute of the day, from 0 to 1439. */
  value: number;
  /** Counts hours 1 to 12 with an AM and PM button. */
  twelve_hour?: boolean;
  gap?: number;
  /** Receives the minute of the day chosen. */
  onChange?: (event: ChangeEvent<number>) => void;
}

/** Rollers for an hour and a minute. Use `TimePicker` to set a time of day held in a signal. */
export function TimeRollers(props: TimeRollersProps) {
  const hour = Math.floor(props.value / 60);
  const minute = props.value % 60;
  const send = (next: number) => props.onChange && run_handler(props.onChange, { value: next });
  return (
    <div data-widget="time-rollers" style={{ display: "flex", gap: `${props.gap ?? 6}px`, justifyContent: "center", alignItems: "center", ...common_style(props) }}>
      <Roller
        options={props.twelve_hour ? two_digit_options(1, 12) : two_digit_options(0, 24)}
        value={props.twelve_hour ? (hour + 11) % 12 : hour}
        onChange={(event) =>
          send(((props.twelve_hour ? ((event.value + 1) % 12) + (hour >= 12 ? 12 : 0) : event.value) * 60) + minute)
        }
      />
      <span style={{ fontSize: `${FONT_SIZES_IN_PIXELS.large}px` }}>:</span>
      <Roller options={two_digit_options(0, 60)} value={minute} onChange={(event) => send(hour * 60 + event.value)} />
      {props.twelve_hour && (
        <Button width={56} onClick={() => send(((hour + 12) % 24) * 60 + minute)}>
          {hour < 12 ? "AM" : "PM"}
        </Button>
      )}
    </div>
  );
}

export interface DurationRollersProps extends CommonProps {
  /** The duration in seconds. */
  value: number;
  show_hours?: boolean;
  gap?: number;
  /** Receives the duration chosen, in seconds. */
  onChange?: (event: ChangeEvent<number>) => void;
}

/** Rollers for minutes and seconds, and hours if asked. Use `DurationPicker` to set a duration held in a signal. */
export function DurationRollers(props: DurationRollersProps) {
  const hours = Math.floor(props.value / 3600);
  const minutes = Math.floor(props.value / 60) % (props.show_hours ? 60 : 100);
  const seconds = props.value % 60;
  const send = (next_hours: number, next_minutes: number, next_seconds: number) =>
    props.onChange && run_handler(props.onChange, { value: next_hours * 3600 + next_minutes * 60 + next_seconds });
  return (
    <div data-widget="duration-rollers" style={{ display: "flex", gap: `${props.gap ?? 6}px`, justifyContent: "center", alignItems: "center", ...common_style(props) }}>
      {props.show_hours && (
        <Roller options={two_digit_options(0, 24)} value={hours} onChange={(event) => send(event.value, minutes, seconds)} />
      )}
      {props.show_hours && <span style={{ fontSize: `${FONT_SIZES_IN_PIXELS.large}px` }}>:</span>}
      <Roller
        options={two_digit_options(0, props.show_hours ? 60 : 100)}
        value={minutes}
        onChange={(event) => send(props.show_hours ? hours : 0, event.value, seconds)}
      />
      <span style={{ fontSize: `${FONT_SIZES_IN_PIXELS.large}px` }}>:</span>
      <Roller options={two_digit_options(0, 60)} value={seconds} onChange={(event) => send(props.show_hours ? hours : 0, minutes, event.value)} />
    </div>
  );
}

export interface DayButtonsProps extends CommonProps {
  /** The days set, Monday in bit 0. */
  value: number;
  /** How many buttons in a row; all seven unless given. */
  columns?: number;
  gap?: number;
  /** Receives the days set after a button is tapped. */
  onChange?: (event: ChangeEvent<number>) => void;
}

const DAY_LETTERS = ["M", "T", "W", "T", "F", "S", "S"];

/** A button for each day of the week, lit while it is set. Use `WeekdayPicker` for days held in a signal. */
export function DayButtons(props: DayButtonsProps) {
  const columns = props.columns ?? 7;
  return (
    <div
      data-widget="day-buttons"
      style={{ display: "grid", gridTemplateColumns: `repeat(${columns}, 1fr)`, gap: `${props.gap ?? 0}px`, ...common_style(props) }}
    >
      {DAY_LETTERS.map((letter, day) => (
        <Button
          background={(props.value & (1 << day)) !== 0 ? "accent" : "surface"}
          onClick={() => props.onChange && run_handler(props.onChange, { value: props.value ^ (1 << day) })}
        >
          {letter}
        </Button>
      ))}
    </div>
  );
}

export interface StepperProps extends CommonProps {
  value: number;
  minimum: number;
  maximum: number;
  step?: number;
  gap?: number;
  /** What shows between the buttons; the value unless given. */
  text?: string;
  /** Receives the value after a step. */
  onChange?: (event: ChangeEvent<number>) => void;
}

/** A minus button, the value and a plus button. */
export function Stepper(props: StepperProps) {
  const step = props.step ?? 1;
  const send = (change: number) =>
    props.onChange &&
    run_handler(props.onChange, { value: Math.min(props.maximum, Math.max(props.minimum, props.value + change)) });
  return (
    <div data-widget="stepper" style={{ display: "flex", gap: `${props.gap ?? 0}px`, alignItems: "center", ...common_style(props) }}>
      <Button onClick={() => send(-step)}>-</Button>
      <Label grow={1} text_align="center">
        {props.text ?? `${props.value}`}
      </Label>
      <Button onClick={() => send(step)}>+</Button>
    </div>
  );
}

export interface ConfirmBoxProps extends CommonProps {
  /** The cancel button's text, the confirm button's and the question, each on a line of its own. */
  text: string;
  /** Receives 1 when the confirm button is tapped and 0 for the other. */
  onChange?: (event: ChangeEvent<number>) => void;
}

/** A question with two buttons over the screen. Use `ConfirmDialog`, which shows while a signal is true. */
export function ConfirmBox(props: ConfirmBoxProps) {
  const send = (value: number) => props.onChange && run_handler(props.onChange, { value });
  const [cancel_text, confirm_text, ...question] = props.text.split("\n");
  return (
    <div data-widget="confirm-box" style={{ display: "flex", flexDirection: "column", gap: "16px", padding: "16px", ...common_style(props) }}>
      <Label text_align="center">{question.join("\n")}</Label>
      <div style={{ display: "flex", gap: "8px" }}>
        <Button grow={1} onClick={() => send(0)}>
          {cancel_text || "No"}
        </Button>
        <Button grow={1} background="danger" onClick={() => send(1)}>
          {confirm_text || "Yes"}
        </Button>
      </div>
    </div>
  );
}

export interface NumberRollerProps extends CommonProps {
  value: number;
  minimum: number;
  maximum: number;
  /** Digits to pad to, such as 2 for minutes. */
  digits?: number;
  /** Receives the number chosen. */
  onChange?: (event: ChangeEvent<number>) => void;
}

/** One roller of whole numbers. Use `NumberPicker` for a number held in a signal. */
export function NumberRoller(props: NumberRollerProps) {
  const options = Array.from({ length: props.maximum - props.minimum + 1 }, (_, place) =>
    `${props.minimum + place}`.padStart(props.digits ?? 1, "0"),
  );
  return (
    <Roller
      {...props}
      options={options}
      value={props.value - props.minimum}
      onChange={(event) => props.onChange && run_handler(props.onChange, { value: event.value + props.minimum })}
    />
  );
}
