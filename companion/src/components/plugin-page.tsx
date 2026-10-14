import { ThemedText } from '@/components/themed-text';
import type { PluginBridge } from '@/services/plugins';

// Plugins run only in the desktop app for now, so a phone has no settings page to show.
export function PluginPage(_props: { id: string; bridge: PluginBridge }) {
  return (
    <ThemedText type="small" themeColor="textSecondary">
      Plugin settings open in the desktop app.
    </ThemedText>
  );
}
