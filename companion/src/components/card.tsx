import type { ComponentChildren } from 'preact';

import { ThemedText } from './themed-text';

// A rounded panel with an optional heading, used to group rows on every screen.
export function Card({ title, children }: { title?: string; children?: ComponentChildren }) {
  return (
    <section class="card">
      {title ? (
        <ThemedText type="smallBold" themeColor="textSecondary" class="card-title">
          {title}
        </ThemedText>
      ) : null}
      {children}
    </section>
  );
}
