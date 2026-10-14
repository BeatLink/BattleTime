import { useEffect, useState } from 'preact/hooks';

import { messageFromPage, messageToPage, pluginPageUrl, type PluginBridge } from '@/services/plugins';

// A plugin's settings page in a sandboxed frame, passing its messages to and from the plugin.
export function PluginPage({ id, bridge }: { id: string; bridge: PluginBridge }) {
  const [frame, setFrame] = useState<HTMLIFrameElement | null>(null);

  useEffect(() => {
    const fromPage = (event: MessageEvent) => {
      if (!frame || event.source !== frame.contentWindow) {
        return;
      }
      const found = messageFromPage(event.data);
      if (found) {
        void bridge.uiMessage(id, found.message);
      }
    };
    window.addEventListener('message', fromPage);
    const stop = bridge.onUi((event) => {
      if (event.id === id) {
        frame?.contentWindow?.postMessage(messageToPage(event.message), '*');
      }
    });
    return () => {
      window.removeEventListener('message', fromPage);
      stop();
    };
  }, [bridge, frame, id]);

  // Scripts only: the page has no origin of its own, so it reaches nothing but its plugin.
  return (
    <iframe
      ref={setFrame}
      class="plugin-frame"
      src={pluginPageUrl(id, bridge.pageBase)}
      sandbox="allow-scripts"
      title={`${id} settings`}
    />
  );
}
