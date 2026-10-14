import type { ComponentChildren } from 'preact';

import { ThemedText } from './themed-text';

// The shared page frame: a title and a column of cards, capped in width on wide windows.
export function Screen({ title, children }: { title: string; children?: ComponentChildren }) {
  return (
    <main class="screen">
      <ThemedText type="subtitle">{title}</ThemedText>
      {children}
    </main>
  );
}
