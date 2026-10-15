// Preact Signals with element writes, loaded by the tests in place of `@preact/signals`.
// A signal holding an array hands out a view whose element and record field writes replace the
// array, so `board.value[cell] = 2` and `alarms.value[0].enabled = true` notify like a whole assignment.

import { signal as plain_signal, useSignal as plain_use_signal, Signal } from "@preact/signals";

export * from "@preact/signals";

// ============================================================================
// Writable array views
// ============================================================================

const signal_value = Object.getOwnPropertyDescriptor(Signal.prototype, "value")!;
const views_of_signals = new WeakMap<Signal<unknown>, WeakMap<unknown[], unknown[]>>();
const arrays_of_views = new WeakMap<unknown[], unknown[]>();

/** The array a view stands for, so a view is never stored inside a signal. */
function unwrapped<Value>(value: Value): Value {
  return (Array.isArray(value) ? arrays_of_views.get(value) ?? value : value) as Value;
}

function writable_view(owner: Signal<unknown>, array: unknown[]): unknown[] {
  let views = views_of_signals.get(owner);
  if (!views) {
    views = new WeakMap();
    views_of_signals.set(owner, views);
  }
  const known = views.get(array);
  if (known) return known;
  const view = new Proxy(array, {
    get(target, property, receiver) {
      const element = Reflect.get(target, property, receiver);
      if (typeof property !== "string" || !/^\d+$/.test(property)) return element;
      if (typeof element !== "object" || element === null || Array.isArray(element)) return element;
      return record_view(owner, target, Number(property), element as Record<string, unknown>);
    },
    set(target, property, element) {
      if (typeof property === "string" && /^\d+$/.test(property)) {
        const copy = target.slice();
        copy[Number(property)] = element;
        signal_value.set!.call(owner, copy);
        return true;
      }
      return Reflect.set(target, property, element);
    },
  });
  views.set(array, view);
  arrays_of_views.set(view, array);
  return view;
}

/** A record inside an array whose field writes replace the array, as element writes do. */
function record_view(
  owner: Signal<unknown>,
  array: unknown[],
  place: number,
  record: Record<string, unknown>,
): Record<string, unknown> {
  return new Proxy(record, {
    set(target, field, value) {
      const copy = array.slice();
      copy[place] = { ...target, [field]: value };
      signal_value.set!.call(owner, copy);
      return true;
    },
  });
}

function with_element_writes<Value>(owner: Signal<Value>): Signal<Value> {
  if (Object.prototype.hasOwnProperty.call(owner, "value")) return owner;
  Object.defineProperty(owner, "value", {
    get(this: Signal<unknown>) {
      const value = signal_value.get!.call(this);
      return Array.isArray(value) ? writable_view(this, value) : value;
    },
    set(this: Signal<unknown>, value: unknown) {
      signal_value.set!.call(this, unwrapped(value));
    },
  });
  return owner;
}

// ============================================================================
// Signals
// ============================================================================

export function signal<Value>(value: Value): Signal<Value>;
export function signal<Value = undefined>(): Signal<Value | undefined>;
export function signal<Value>(value?: Value): Signal<Value | undefined> {
  return with_element_writes(plain_signal(unwrapped(value)));
}

export function useSignal<Value>(value: Value): Signal<Value>;
export function useSignal<Value = undefined>(): Signal<Value | undefined>;
export function useSignal<Value>(value?: Value): Signal<Value | undefined> {
  return with_element_writes(plain_use_signal(unwrapped(value)));
}
