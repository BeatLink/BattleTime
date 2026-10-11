// Preact Signals with element writes, loaded by the tests in place of `@preact/signals`.
// A signal holding an array hands out a view whose element writes replace the array, so
// `board.value[cell] = 2` notifies like a whole assignment and later reads see the write.

import { signal as plain_signal, useSignal as plain_use_signal, Signal } from "@preact/signals";

export * from "@preact/signals";

// ============================================================================
// Writable array views
// ============================================================================

const signal_value = Object.getOwnPropertyDescriptor(Signal.prototype, "value")!;
const views_of_arrays = new WeakMap<unknown[], unknown[]>();
const arrays_of_views = new WeakMap<unknown[], unknown[]>();

function writable_view(owner: Signal<unknown>, array: unknown[]): unknown[] {
  const known = views_of_arrays.get(array);
  if (known) return known;
  const view = new Proxy(array, {
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
  views_of_arrays.set(array, view);
  arrays_of_views.set(view, array);
  return view;
}

function with_element_writes<Value>(owner: Signal<Value>): Signal<Value> {
  if (Object.prototype.hasOwnProperty.call(owner, "value")) return owner;
  Object.defineProperty(owner, "value", {
    get(this: Signal<unknown>) {
      const value = signal_value.get!.call(this);
      return Array.isArray(value) ? writable_view(this, value) : value;
    },
    set(this: Signal<unknown>, value: unknown) {
      const array = Array.isArray(value) ? arrays_of_views.get(value) : undefined;
      signal_value.set!.call(this, array ?? value);
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
  return with_element_writes(plain_signal(value));
}

export function useSignal<Value>(value: Value): Signal<Value>;
export function useSignal<Value = undefined>(): Signal<Value | undefined>;
export function useSignal<Value>(value?: Value): Signal<Value | undefined> {
  return with_element_writes(plain_use_signal(value));
}
