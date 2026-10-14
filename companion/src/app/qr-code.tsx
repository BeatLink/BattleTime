import { useMemo, useState } from 'react';
import { StyleSheet, Switch, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, TextRow } from '@/components/fields';
import { QrPreview } from '@/components/qr-preview';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import {
  cleanQrLabel,
  DEFAULT_QR_LEVEL,
  LARGEST_QR_LABEL_BYTES,
  qrBitmap,
  qrDensityWarning,
  QR_LEVELS,
  removeQrCode,
  sendQrCode,
  WIFI_SECURITIES,
  wifiText,
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

// Makes a QR code from text or a Wi-Fi network, previews it, and sends it to the watch's qr-code app.
export default function QrCodeScreen() {
  const theme = useTheme();
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

  const send = () =>
    task.run('Sending the code', async () => {
      if ('bitmap' in encoded) {
        await sendQrCode(await files(), encoded.bitmap, label);
        setSent(caption ? `Sent, captioned "${caption}".` : 'Sent.');
      }
    });

  const remove = () =>
    task.run('Removing the code', async () => {
      await removeQrCode(await files());
      setSent('Removed from the watch.');
    });

  return (
    <Screen title="QR code">
      <Card title="Code">
        <ThemedText type="small" themeColor="textSecondary">
          The watch&apos;s QR code app shows one code, for a link, some text or a Wi-Fi network a phone&apos;s
          camera can join.
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
              <TextRow label="Password" value={password} onChangeText={setPassword} secureTextEntry />
            ) : null}
            <View style={styles.switchRow}>
              <ThemedText>Hidden network</ThemedText>
              <Switch
                value={hidden}
                onValueChange={setHidden}
                trackColor={{ true: theme.accent, false: theme.backgroundSelected }}
                thumbColor={theme.text}
                accessibilityLabel="Hidden network"
              />
            </View>
          </>
        )}
        <TextRow
          label="Caption"
          detail={`Shown under the code on the watch: up to ${LARGEST_QR_LABEL_BYTES} plain characters, accents dropped.`}
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

      <Card title="Watch">
        {connected ? (
          <>
            <Button
              title="Send to the watch"
              disabled={task.busy !== null || !('bitmap' in encoded)}
              busy={task.busy === 'Sending the code'}
              onPress={() => void send()}
            />
            <Button
              title="Remove it from the watch"
              variant="secondary"
              disabled={task.busy !== null}
              onPress={() => void remove()}
            />
            {sent ? (
              <ThemedText type="small" themeColor="textSecondary">
                {sent}
              </ThemedText>
            ) : null}
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Connect a watch that serves files to send the code.
          </ThemedText>
        )}
      </Card>

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  switchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Spacing.two,
  },
});
