import { useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, TextRow } from '@/components/fields';
import { IconPicker, LibraryIcon } from '@/components/library-icon';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { BUILTIN_ACTIONS, HTTP_METHODS, MOST_REMOTE_BUTTONS, type RemoteActionKind } from '@/protocol/remote';
import { BUILTIN_LABELS, buttonName, describeAction } from '@/services/remote-actions';
import {
  buttonFromForm,
  EMPTY_FORM,
  formFromButton,
  moveItem,
  type AndroidIntentType,
  type RemoteButtonForm,
} from '@/state/remote-form';
import type { RemoteStatus } from '@/state/remote-sync';
import { useTask } from '@/state/use-task';

const KIND_CHOICES: { value: RemoteActionKind; label: string }[] = [
  { value: 'builtin', label: 'Ready-made' },
  { value: 'http', label: 'Web request' },
  { value: 'command', label: 'Command' },
  { value: 'android', label: 'Android' },
];

const INTENT_CHOICES: { value: AndroidIntentType; label: string }[] = [
  { value: 'deeplink', label: 'Open a link' },
  { value: 'activity', label: 'Start an activity' },
  { value: 'broadcast', label: 'Broadcast' },
];

const METHOD_CHOICES = HTTP_METHODS.map((method) => ({ value: method, label: method }));

const BUILTIN_CHOICES = BUILTIN_ACTIONS.map((name) => ({ value: name, label: BUILTIN_LABELS[name] }));

const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

// One line on how keeping the watch's layout up to date is going.
function describeStatus(status: RemoteStatus, connected: boolean): string {
  if (!connected) {
    return 'The buttons are sent when a watch connects.';
  }
  switch (status.kind) {
    case 'idle':
      return 'Not sent yet.';
    case 'sending':
      return `Sending ${status.buttons} buttons…`;
    case 'synced':
      return `On the watch, ${status.buttons} buttons; checked at ${clock(status.at)}.`;
    case 'failed':
      return status.message;
  }
}

// The fields for one button's action, by kind.
function ActionFields({ form, set }: { form: RemoteButtonForm; set: (patch: Partial<RemoteButtonForm>) => void }) {
  switch (form.kind) {
    case 'builtin':
      return <ChoiceRow label="Action" options={BUILTIN_CHOICES} value={form.builtin} onChange={(builtin) => set({ builtin })} />;
    case 'http':
      return (
        <>
          <ChoiceRow label="Method" options={METHOD_CHOICES} value={form.method} onChange={(method) => set({ method })} />
          <TextRow label="Address" placeholder="https://example.com/hook" autoCapitalize="none" value={form.url} onChangeText={(url) => set({ url })} />
          <TextRow label="Headers" detail="One Name: value per line." multiline autoCapitalize="none" value={form.headers} onChangeText={(headers) => set({ headers })} />
          {form.method !== 'GET' ? (
            <TextRow label="Body" multiline autoCapitalize="none" value={form.body} onChangeText={(body) => set({ body })} />
          ) : null}
        </>
      );
    case 'command':
      return (
        <TextRow
          label="Command"
          detail="Run in your shell on this computer, stopped after 30 seconds."
          placeholder="notify-send hello"
          autoCapitalize="none"
          value={form.command}
          onChangeText={(command) => set({ command })}
        />
      );
    case 'android':
      return (
        <>
          <ChoiceRow label="Intent" options={INTENT_CHOICES} value={form.intentType} onChange={(intentType) => set({ intentType })} />
          {form.intentType === 'deeplink' ? (
            <TextRow label="Link" placeholder="geo:0,0?q=coffee" autoCapitalize="none" value={form.uri} onChangeText={(uri) => set({ uri })} />
          ) : null}
          {form.intentType !== 'deeplink' ? (
            <>
              <TextRow
                label={form.intentType === 'activity' ? 'Package' : 'Package (optional)'}
                placeholder="com.example.app"
                autoCapitalize="none"
                value={form.packageName}
                onChangeText={(packageName) => set({ packageName })}
              />
              {form.intentType === 'activity' ? (
                <TextRow label="Activity class" placeholder="com.example.app.MainActivity" autoCapitalize="none" value={form.className} onChangeText={(className) => set({ className })} />
              ) : null}
              <TextRow
                label={form.intentType === 'activity' ? 'Action (optional)' : 'Action'}
                placeholder="com.example.app.ACTION"
                autoCapitalize="none"
                value={form.intentAction}
                onChangeText={(intentAction) => set({ intentAction })}
              />
              <TextRow label="Extras" detail="One key=value per line, sent as strings." multiline autoCapitalize="none" value={form.extras} onChangeText={(extras) => set({ extras })} />
            </>
          ) : null}
        </>
      );
  }
}

export default function RemoteScreen() {
  const theme = useTheme();
  const {
    settings,
    updateSettings,
    watchReady,
    capabilities,
    remoteStatus,
    remoteLog,
    clearRemoteLog,
    sendRemoteLayout,
    testRemoteButton,
    remoteUnavailable,
    simulateRemoteTap,
  } = useWatch();
  const task = useTask();
  const buttons = settings.remoteButtons;
  const connected = watchReady && capabilities.files;
  // The button being edited: its place, or the end of the list for a new one, with its fields.
  const [editing, setEditing] = useState<{ index: number; form: RemoteButtonForm } | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const setForm = (patch: Partial<RemoteButtonForm>) =>
    setEditing((current) => (current ? { ...current, form: { ...current.form, ...patch } } : current));

  const save = () => {
    if (!editing) return;
    try {
      const button = buttonFromForm(editing.form);
      const next = [...buttons];
      next[editing.index] = button;
      void updateSettings({ remoteButtons: next });
      setEditing(null);
      setFormError(null);
    } catch (error) {
      setFormError((error as Error).message);
    }
  };

  const remove = (index: number) => void updateSettings({ remoteButtons: buttons.filter((_button, at) => at !== index) });
  const move = (index: number, step: number) => void updateSettings({ remoteButtons: moveItem(buttons, index, step) });

  return (
    <Screen title="Remote">
      <Card title="Buttons">
        <ThemedText type="small" themeColor="textSecondary">
          The watch&apos;s Remote app shows up to {MOST_REMOTE_BUTTONS} buttons. A tap runs the button&apos;s action
          here; the watch only knows each button&apos;s icon.
        </ThemedText>
        {buttons.length === 0 ? <ThemedText>No buttons yet.</ThemedText> : null}
        {buttons.map((button, index) => {
          const reason = remoteUnavailable(button.action);
          return (
            <View key={index} style={[styles.button, { borderColor: theme.backgroundSelected }]}>
              <View style={styles.buttonHeader}>
                <LibraryIcon name={button.icon} />
                <View style={styles.buttonText}>
                  <ThemedText>{`${index + 1}. ${buttonName(button)}`}</ThemedText>
                  <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
                    {describeAction(button.action)}
                  </ThemedText>
                  {reason ? (
                    <ThemedText type="small" style={{ color: theme.danger }}>
                      Unavailable here: {reason}
                    </ThemedText>
                  ) : null}
                </View>
              </View>
              <View style={styles.actions}>
                <Button title="Edit" variant="secondary" onPress={() => setEditing({ index, form: formFromButton(button) })} />
                <Button title="Up" variant="secondary" disabled={index === 0} onPress={() => move(index, -1)} />
                <Button title="Down" variant="secondary" disabled={index === buttons.length - 1} onPress={() => move(index, 1)} />
                <Button title="Test" variant="secondary" disabled={reason !== null} onPress={() => void testRemoteButton(index)} />
                {simulateRemoteTap ? (
                  <Button title="Tap as the watch" variant="secondary" onPress={() => simulateRemoteTap(index)} />
                ) : null}
                <Button title="Remove" variant="danger" onPress={() => remove(index)} />
              </View>
            </View>
          );
        })}
        {editing === null ? (
          <Button
            title="Add a button"
            disabled={buttons.length >= MOST_REMOTE_BUTTONS}
            onPress={() => setEditing({ index: buttons.length, form: { ...EMPTY_FORM } })}
          />
        ) : null}
      </Card>

      {editing ? (
        <Card title={editing.index < buttons.length ? `Button ${editing.index + 1}` : 'New button'}>
          <TextRow
            label="Label"
            detail="Shown only here; the watch shows the icon."
            value={editing.form.label}
            onChangeText={(label) => setForm({ label })}
          />
          <ThemedText type="small" themeColor="textSecondary">
            Icon: {editing.form.icon}
          </ThemedText>
          <IconPicker value={editing.form.icon} onChange={(icon) => setForm({ icon })} />
          <ChoiceRow label="What it does" options={KIND_CHOICES} value={editing.form.kind} onChange={(kind) => setForm({ kind })} />
          <ActionFields form={editing.form} set={setForm} />
          <ErrorNote error={formError} />
          <View style={styles.actions}>
            <Button title="Save" onPress={save} />
            <Button
              title="Cancel"
              variant="secondary"
              onPress={() => {
                setEditing(null);
                setFormError(null);
              }}
            />
          </View>
        </Card>
      ) : null}

      <Card title="On the watch">
        <ThemedText type="small" themeColor={remoteStatus.kind === 'failed' && connected ? 'danger' : 'textSecondary'}>
          {describeStatus(remoteStatus, connected)}
        </ThemedText>
        <Button
          title="Send to watch"
          disabled={!connected || task.busy !== null}
          onPress={() => void task.run('Sending the buttons', sendRemoteLayout)}
        />
      </Card>

      <Card title="Recent taps">
        {remoteLog.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            Nothing tapped yet.
          </ThemedText>
        ) : null}
        {remoteLog.map((entry, index) => (
          <ThemedText key={`${entry.at}-${index}`} type="small" style={{ color: entry.ok ? theme.text : theme.danger }}>
            {`${clock(entry.at)} ${entry.source === 'test' ? 'test' : 'watch'} · ${entry.label}: ${entry.message}`}
          </ThemedText>
        ))}
        {remoteLog.length ? <Button title="Clear" variant="secondary" onPress={clearRemoteLog} /> : null}
      </Card>

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  button: { gap: Spacing.two, paddingVertical: Spacing.two, borderTopWidth: StyleSheet.hairlineWidth },
  buttonHeader: { flexDirection: 'row', alignItems: 'center', gap: Spacing.three },
  buttonText: { flex: 1 },
  actions: { flexDirection: 'row', flexWrap: 'wrap', gap: Spacing.two },
});
