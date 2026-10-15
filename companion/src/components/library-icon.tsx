import { useMemo } from 'preact/hooks';

import { LIBRARY_ICON_NAMES } from '@/bundles/icon-names';
import { libraryIconUris } from '@/protocol/library-icons';

import { ThemedText } from './themed-text';

// A library icon as the watch draws it, tinted in the text colour, or its name where the bundle has no picture.
export function LibraryIcon({ name, size = 28 }: { name: string; size?: number }) {
  const uris = useMemo(() => libraryIconUris(), []);
  const uri = uris[(LIBRARY_ICON_NAMES as readonly string[]).indexOf(name)];
  if (!uri) {
    return (
      <ThemedText type="small" oneLine style={{ width: `${size * 2}px` }}>
        {name}
      </ThemedText>
    );
  }
  // The icon is a white mask, so the theme's text colour shows through it.
  const mask = `url("${uri}")`;
  return (
    <span
      class="library-icon"
      role="img"
      aria-label={name}
      style={{ width: `${size}px`, height: `${size}px`, maskImage: mask, WebkitMaskImage: mask }}
    />
  );
}

// A grid of every library icon; the chosen one is outlined in the accent colour.
export function IconPicker({ value, onChange }: { value: string; onChange: (name: string) => void }) {
  return (
    <div class="icon-grid">
      {LIBRARY_ICON_NAMES.filter((name) => name !== 'none').map((name) => (
        <button
          type="button"
          key={name}
          aria-label={name}
          aria-pressed={name === value}
          class={name === value ? 'icon-cell selected' : 'icon-cell'}
          onClick={() => onChange(name)}>
          <LibraryIcon name={name} />
        </button>
      ))}
    </div>
  );
}
