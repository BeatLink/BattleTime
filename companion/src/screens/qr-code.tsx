import { useEffect, useMemo, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, TextRow } from '@/components/fields';
import { QrPreview } from '@/components/qr-preview';
import { ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import {
  addQrCode,
  cleanQrLabel,
  DEFAULT_QR_LEVEL,
  LARGEST_QR_LABEL_BYTES,
  MOST_QR_CODES,
  moveQrCode,
  qrBitmap,
  qrDensityWarning,
  QR_LEVELS,
  readQrList,
  removeAllQrCodes,
  removeQrCode,
  renameQrCode,
  suggestedQrLabel,
  WIFI_SECURITIES,
  wifiText,
  type QrEntry,
  type QrLevel,
  type WifiSecurity,
} from '@/protocol/qr-code';
import { useTask } from '@/state/use-task';

// ============================================================================
// Choices
// ============================================================================

type QrKind = 'text' | 'wifi';

const KIND_CHOICES: { value: QrKind; label: string }[] = [
  { value: 'text', label: 'Text or link' },
  { value: 'wifi', label: 'Wi-Fi network' },
];

const LEVEL_NAMES: Record<QrLevel, string> = { L: 'Low', M: 'Medium', Q: 'Quartile', H: 'High' };
const LEVEL_CHOICES = QR_LEVELS.map((level) => ({ value: level, label: LEVEL_NAMES[level] }));

const SECURITY_NAMES: Record<WifiSecurity, string> = { WPA: 'WPA', WEP: 'WEP', nopass: 'None' };
const SECURITY_CHOICES = WIFI_SECURITIES.map((security) => ({ value: security, label: SECURITY_NAMES[security] }));

// The code for the form as it stands, or why there is none yet.
type Encoded = { bitmap: Uint8Array; warning: string | null } | { problem: string };

// ============================================================================
// The screen
// ============================================================================
// Makes a QR code from text or a Wi-Fi network, previews it and adds it to the watch's qr-code app, and keeps the
// list of codes on the watch: renaming, reordering and removing them.
export default function QrCodeScreen() {
  const { files, watchReady, capabilities } = useWatch();
  const task = useTask();
  const connected = watchReady && capabilities.files;

  const [kind, setKind] = useState<QrKind>('text');
  const [text, setText] = useState('');
  const [ssid, setSsid] = useState('');
  const [password, setPassword] = useState('');
  const [security, setSecurity] = useState<WifiSecurity>('WPA');
  const [hidden, setHidden] = useState(false);
  const [label, setLabel] = useState('');
  const [level, setLevel] = useState<QrLevel>(DEFAULT_QR_LEVEL);
  const [sent, setSent] = useState<string | null>(null);
  // The codes on the watch, or null until they are read.
  const [entries, setEntries] = useState<QrEntry[] | null>(null);
  // The slot of the code being renamed, with the caption typed so far.
  const [renaming, setRenaming] = useState<{ slot: number; label: string } | null>(null);

  const encoded = useMemo<Encoded>(() => {
    try {
      const content = kind === 'wifi' ? wifiText({ ssid, password, security, hidden }) : text;
      if (content === '') {
        return { problem: 'Type the text or link to put in the code.' };
      }
      const bitmap = qrBitmap(content, level);
      return { bitmap, warning: qrDensityWarning(bitmap) };
    } catch (error) {
      return { problem: (error as Error).message };
    }
  }, [kind, text, ssid, password, security, hidden, level]);

  const caption = cleanQrLabel(label);
  const suggestion = suggestedQrLabel(kind === 'wifi' ? ssid : text);
  const isFull = entries !== null && entries.length >= MOST_QR_CODES;

  // Runs a change to the watch's list under a label and shows the list it leaves.
  const change = (busyLabel: string, action: () => Promise<QrEntry[]>) =>
    task.run(busyLabel, async () => setEntries(await action()));

  useEffect(() => {
    if (!connected) {
      return;
    }
    void task.run('Reading the codes', async () => setEntries(await readQrList(await files())));
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected]);

  const send = () =>
    change('Sending the code', async () => {
      if (!('bitmap' in encoded)) {
        return entries ?? [];
      }
      const added = await addQrCode(await files(), encoded.bitmap, caption || suggestion);
      setSent(`Added as number ${added.length}, "${added[added.length - 1].label}".`);
      return added;
    });

  const rename = () =>
    renaming &&
    change('Renaming the code', async () => {
      const renamed = await renameQrCode(await files(), renaming.slot, renaming.label);
      setRenaming(null);
      return renamed;
    });

  const removeAll = () =>
    change('Removing every code', async () => {
      await removeAllQrCodes(await files());
      return [];
    });

  return (
    <Screen title="QR code">
      <Card title="Code">
        <ThemedText type="small" themeColor="textSecondary">
          The watch&apos;s QR code app lists up to {MOST_QR_CODES} codes, each for a link, some text or a Wi-Fi network
          a phone&apos;s camera can join.
        </ThemedText>
        <ChoiceRow<QrKind> label="What it holds" options={KIND_CHOICES} value={kind} onChange={setKind} />
        {kind === 'text' ? (
          <TextRow
            label="Text or link"
            placeholder="https://example.org"
            value={text}
            onChangeText={setText}
            multiline
          />
        ) : (
          <>
            <TextRow label="Network name" value={ssid} onChangeText={setSsid} />
            <ChoiceRow<WifiSecurity> label="Security" options={SECURITY_CHOICES} value={security} onChange={setSecurity} />
            {security !== 'nopass' ? (
              <TextRow label="Password" value={password} onChangeText={setPassword} password />
            ) : null}
            <ToggleRow label="Hidden network" value={hidden} onValueChange={setHidden} />
          </>
        )}
        <TextRow
          label="Caption"
          detail={`Names the code in the watch's list and shows under it: up to ${LARGEST_QR_LABEL_BYTES} plain characters, accents dropped.`}
          placeholder={suggestion}
          value={label}
          onChangeText={setLabel}
        />
        {caption !== label.trim() && caption !== '' ? (
          <ThemedText type="small" themeColor="textSecondary">
            The watch shows: {caption}
          </ThemedText>
        ) : null}
        <ChoiceRow<QrLevel>
          label="Error correction"
          detail="Higher survives more damage but makes a bigger, denser code."
          options={LEVEL_CHOICES}
          value={level}
          onChange={setLevel}
        />
      </Card>

      <Card title="Preview">
        {'bitmap' in encoded ? (
          <>
            <QrPreview bitmap={encoded.bitmap} />
            <ThemedText type="small" themeColor="textSecondary">
              {encoded.bitmap[0]} modules wide.
            </ThemedText>
            {encoded.warning ? (
              <ThemedText type="small" themeColor="danger">
                {encoded.warning}
              </ThemedText>
            ) : null}
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            {encoded.problem}
          </ThemedText>
        )}
      </Card>
      <Card title="On the watch">
        {connected ? (
          <>
            <Button
              title="Add to the watch"
              disabled={task.busy !== null || !('bitmap' in encoded) || isFull}
              busy={task.busy === 'Sending the code'}
              onClick={() => void send()}
            />
            {isFull ? (
              <ThemedText type="small" themeColor="textSecondary">
                The watch holds {MOST_QR_CODES} codes; remove one to add another.
              </ThemedText>
            ) : null}
            {sent ? (
              <ThemedText type="small" themeColor="textSecondary">
                {sent}
              </ThemedText>
            ) : null}
            {entries?.length === 0 ? <ThemedText>No codes on the watch yet.</ThemedText> : null}
            {entries?.map((entry, place) => (
              <div key={entry.slot} class="entry">
                {renaming?.slot === entry.slot ? (
                  <>
                    <TextRow
                      label={`Caption for number ${place + 1}`}
                      value={renaming.label}
                      onChangeText={(typed) => setRenaming({ slot: entry.slot, label: typed })}
                    />
                    <div class="actions">
                      <Button title="Save" disabled={task.busy !== null} onClick={() => void rename()} />
                      <Button title="Cancel" variant="secondary" onClick={() => setRenaming(null)} />
                    </div>
                  </>
                ) : (
                  <>
                    <ThemedText>{`${place + 1}. ${entry.label}`}</ThemedText>
                    <div class="actions">
                      <Button
                        title="Rename"
                        variant="secondary"
                        disabled={task.busy !== null}
                        onClick={() => setRenaming({ slot: entry.slot, label: entry.label })}
                      />
                      <Button
                        title="Up"
                        variant="secondary"
                        disabled={task.busy !== null || place === 0}
                        onClick={() =>
                          void change('Moving the code', async () => moveQrCode(await files(), entry.slot, -1))
                        }
                      />
                      <Button
                        title="Down"
                        variant="secondary"
                        disabled={task.busy !== null || place === entries.length - 1}
                        onClick={() =>
                          void change('Moving the code', async () => moveQrCode(await files(), entry.slot, 1))
                        }
                      />
                      <Button
                        title="Remove"
                        variant="danger"
                        disabled={task.busy !== null}
                        onClick={() =>
                          void change('Removing the code', async () => removeQrCode(await files(), entry.slot))
                        }
                      />
                    </div>
                  </>
                )}
              </div>
            ))}
            {entries && entries.length > 0 ? (
              <Button
                title="Remove every code"
                variant="secondary"
                disabled={task.busy !== null}
                onClick={() => void removeAll()}
              />
            ) : null}
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Connect a watch that serves files to add codes and see the ones it holds.
          </ThemedText>
        )}
      </Card>

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
