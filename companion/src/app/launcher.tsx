import { useEffect, useState } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, TextRow } from '@/components/fields';
import { IconPicker, LibraryIcon } from '@/components/library-icon';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import {
  DEFAULT_LAUNCHER,
  groupApps,
  MOST_GROUPS,
  newGroupId,
  OTHER_GROUP_ID,
  placeApp,
  readLauncher,
  removeGroup,
  writeLauncher,
  type LauncherSettings,
  type LauncherStyle,
  type LauncherView,
} from '@/protocol/launcher';
import type { WatchApp } from '@/protocol/packages';
import { moveItem } from '@/state/remote-form';
import { useTask } from '@/state/use-task';

const STYLE_CHOICES: { value: LauncherStyle; label: string }[] = [
  { value: 'folders', label: 'Folders' },
  { value: 'sections', label: 'Sections' },
  { value: 'pages', label: 'Pages' },
  { value: 'list', label: 'One list' },
];

const STYLE_DETAILS: Record<LauncherStyle, string> = {
  folders: 'A list of groups; tap one to see its apps.',
  sections: 'Every app in one list, with each group’s name above its apps.',
  pages: 'One page per group; swipe left and right between them.',
  list: 'Every app in one list, without groups.',
};

const VIEW_CHOICES: { value: LauncherView; label: string }[] = [
  { value: 'list', label: 'List' },
  { value: 'grid', label: 'Grid' },
];

// Arranges the watch's launcher: how it shows the groups, what the groups are, and which apps each holds.
export default function LauncherScreen() {
  const theme = useTheme();
  const { files, packages, watchReady, capabilities } = useWatch();
  const task = useTask();
  const [settings, setSettings] = useState<LauncherSettings | null>(null);
  const [apps, setApps] = useState<WatchApp[]>([]);
  const [saved, setSaved] = useState(true);
  const [editing, setEditing] = useState<string | null>(null);
  const [chosenApp, setChosenApp] = useState<string | null>(null);
  const [newName, setNewName] = useState('');

  const ready = watchReady && capabilities.files;

  useEffect(() => {
    if (!ready) return;
    void task.run('Reading the launcher', async () => {
      const [onWatch, listing] = await Promise.all([readLauncher(await files()), (await packages()).apps()]);
      setSettings(onWatch.settings);
      setApps(listing);
      setSaved(onWatch.onWatch);
    });
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  if (!ready) {
    return (
      <Screen title="Launcher">
        <Card>
          <ThemedText themeColor="textSecondary">Connect a watch to arrange its launcher.</ThemedText>
        </Card>
      </Screen>
    );
  }
  if (!settings) {
    return (
      <Screen title="Launcher">
        <ErrorNote error={task.error} onDismiss={task.clearError} />
        <ThemedText themeColor="textSecondary">Reading the watch’s launcher…</ThemedText>
      </Screen>
    );
  }

  const change = (next: LauncherSettings) => {
    setSettings(next);
    setSaved(false);
  };
  const grouped = groupApps(settings, apps);
  const placeOf = (id: string) => settings.groups.findIndex((group) => group.id === id);
  const moveGroup = (id: string, by: number) =>
    change({ ...settings, groups: moveItem(settings.groups, placeOf(id), by) });
  const editGroup = (id: string, patch: { name?: string; icon?: LauncherSettings['groups'][number]['icon'] }) =>
    change({ ...settings, groups: settings.groups.map((group) => (group.id === id ? { ...group, ...patch } : group)) });
  const addGroup = () => {
    const name = newName.trim();
    if (!name || settings.groups.length >= MOST_GROUPS) return;
    const id = newGroupId(settings, name);
    change({ ...settings, groups: [...settings.groups, { id, name, icon: 'pine_logo' }] });
    setNewName('');
    setEditing(id);
  };
  const save = () =>
    task.run('Sending the launcher', async () => {
      await writeLauncher(await files(), await packages(), settings);
      setSaved(true);
    });

  return (
    <Screen title="Launcher">
      <ErrorNote error={task.error} onDismiss={task.clearError} />

      <Card title="On the watch">
        <ChoiceRow label="Style" detail={STYLE_DETAILS[settings.style]} options={STYLE_CHOICES} value={settings.style} onChange={(style) => change({ ...settings, style })} />
        <ChoiceRow label="Apps as" options={VIEW_CHOICES} value={settings.view} onChange={(view) => change({ ...settings, view })} />
        <View style={styles.actions}>
          <Button title={saved ? 'Sent' : 'Send to watch'} busy={task.busy !== null} disabled={saved} onPress={save} />
          <Button title="Start over" variant="secondary" onPress={() => change(DEFAULT_LAUNCHER)} />
        </View>
      </Card>

      {grouped.map(({ group, apps: members }) => {
        const place = placeOf(group.id);
        const isOther = group.id === OTHER_GROUP_ID;
        return (
          <Card key={group.id}>
            <View style={styles.groupHeader}>
              <LibraryIcon name={group.icon} />
              <ThemedText type="subtitle" style={styles.grow}>
                {group.name}
              </ThemedText>
              <ThemedText themeColor="textSecondary">{members.length}</ThemedText>
            </View>
            {place >= 0 ? (
              <View style={styles.actions}>
                <Button title="Up" variant="secondary" disabled={place === 0} onPress={() => moveGroup(group.id, -1)} />
                <Button title="Down" variant="secondary" disabled={place === settings.groups.length - 1} onPress={() => moveGroup(group.id, 1)} />
                <Button title={editing === group.id ? 'Done' : 'Edit'} variant="secondary" onPress={() => setEditing(editing === group.id ? null : group.id)} />
                {isOther ? null : <Button title="Remove" variant="danger" onPress={() => change(removeGroup(settings, group.id))} />}
              </View>
            ) : null}
            {editing === group.id ? (
              <View style={styles.editor}>
                <TextRow label="Name" value={group.name} onChangeText={(name) => editGroup(group.id, { name })} />
                <IconPicker value={group.icon} onChange={(icon) => editGroup(group.id, { icon: icon as typeof group.icon })} />
              </View>
            ) : null}
            {members.length === 0 ? <ThemedText themeColor="textSecondary">No apps; the watch hides an empty group.</ThemedText> : null}
            {members.map((member, index) => (
              <View key={member.id} style={[styles.app, { borderColor: theme.backgroundSelected }]}>
                <Pressable accessibilityRole="button" onPress={() => setChosenApp(chosenApp === member.id ? null : member.id)}>
                  <ThemedText style={chosenApp === member.id ? { color: theme.accent } : undefined}>{member.enabled ? member.name : `${member.name} (hidden)`}</ThemedText>
                </Pressable>
                {chosenApp === member.id ? (
                  <>
                    <View style={styles.actions}>
                      <Button title="Earlier" variant="secondary" disabled={index === 0} onPress={() => change(placeApp(settings, apps, member.id, group.id, index - 1))} />
                      <Button title="Later" variant="secondary" disabled={index === members.length - 1} onPress={() => change(placeApp(settings, apps, member.id, group.id, index + 1))} />
                    </View>
                    <ChoiceRow
                      label="Move to"
                      options={grouped.filter((entry) => entry.group.id !== group.id).map((entry) => ({ value: entry.group.id, label: entry.group.name }))}
                      value={undefined}
                      onChange={(target) => {
                        change(placeApp(settings, apps, member.id, target));
                        setChosenApp(null);
                      }}
                    />
                  </>
                ) : null}
              </View>
            ))}
          </Card>
        );
      })}

      <Card title="New group">
        <TextRow label="Name" placeholder="Fitness" value={newName} onChangeText={setNewName} />
        <Button title="Add group" disabled={!newName.trim() || settings.groups.length >= MOST_GROUPS} onPress={addGroup} />
      </Card>
    </Screen>
  );
}

const styles = StyleSheet.create({
  actions: { flexDirection: 'row', flexWrap: 'wrap', gap: Spacing.two },
  groupHeader: { flexDirection: 'row', alignItems: 'center', gap: Spacing.three },
  grow: { flex: 1 },
  editor: { gap: Spacing.two },
  app: { gap: Spacing.two, paddingVertical: Spacing.two, borderTopWidth: StyleSheet.hairlineWidth },
});
