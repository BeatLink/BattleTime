import { useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';

// Shows the UART traffic in both directions and sends a typed line to the watch as it is.
export default function ConsoleScreen() {
  const { console: entries, connection, sendRaw, clearConsole } = useWatch();
  const [draft, setDraft] = useState('');
  const connected = connection === 'connected';

  const submit = () => {
    const line = draft.trim();
    if (!line) return;
    void sendRaw(`${line}\n`);
    setDraft('');
  };

  return (
    <Screen title="Console">
      <Card>
        {entries.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            Nothing yet. Traffic appears here once a watch is connected.
          </ThemedText>
        ) : (
          entries.map((entry) => (
            <ThemedText key={entry.id} type="code" themeColor={entry.direction === 'out' ? 'accent' : 'text'}>
              {entry.direction === 'out' ? '> ' : '< '}
              {entry.text}
            </ThemedText>
          ))
        )}
      </Card>
      <div class="line" style={{ gap: '8px' }}>
        <input
          class="input mono grow"
          style={{ minHeight: '48px', background: 'var(--background-element)' }}
          value={draft}
          onInput={(event) => setDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
          }}
          placeholder={'Send a line, such as GB({"t":"status"})'}
          aria-label="Line to send"
          autoCapitalize="none"
          autoComplete="off"
          spellcheck={false}
          disabled={!connected}
        />
        <Button title="Send" onClick={submit} disabled={!connected} />
      </div>
      <Button title="Clear" variant="secondary" onClick={clearConsole} />
    </Screen>
  );
}
