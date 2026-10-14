import { useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, TextRow } from '@/components/fields';
import { IconPicker, LibraryIcon } from '@/components/library-icon';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { theme } from '@/constants/theme';
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
  sortLauncher,
  writeLauncher,
  type LauncherSettings,
  type LauncherSort,
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

const SORT_CHOICES: { value: LauncherSort; label: string }[] = [
  { value: 'name', label: 'By name' },
  { value: 'custom', label: 'Custom' },
];

const SORT_DETAILS: Record<LauncherSort, string> = {
  name: 'Groups, and the apps in each, in order of their names.',
  custom: 'Groups and apps in the order you put them.',
};

// Arranges the watch's launcher: how it shows the groups, what the groups are, and which apps each holds.
export default function LauncherScreen() {
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
        <ChoiceRow label="Sort" detail={SORT_DETAILS[settings.sort]} options={SORT_CHOICES} value={settings.sort} onChange={(sort) => change(sortLauncher(settings, apps, sort))} />
        <div class="actions">
          <Button title={saved ? 'Sent' : 'Send to watch'} busy={task.busy !== null} disabled={saved} onClick={save} />
          <Button title="Start over" variant="secondary" onClick={() => change(DEFAULT_LAUNCHER)} />
        </div>
      </Card>

      {grouped.map(({ group, apps: members }) => {
        const place = placeOf(group.id);
        const isOther = group.id === OTHER_GROUP_ID;
        const custom = settings.sort === 'custom';
        return (
          <Card key={group.id}>
            <div class="line">
              <LibraryIcon name={group.icon} />
              <ThemedText type="subtitle" class="grow">
                {group.name}
              </ThemedText>
              <ThemedText themeColor="textSecondary">{members.length}</ThemedText>
            </div>
            {place >= 0 ? (
              <div class="actions">
                {custom ? (
                  <>
                    <Button title="Up" variant="secondary" disabled={place === 0} onClick={() => moveGroup(group.id, -1)} />
                    <Button title="Down" variant="secondary" disabled={place === settings.groups.length - 1} onClick={() => moveGroup(group.id, 1)} />
                  </>
                ) : null}
                <Button title={editing === group.id ? 'Done' : 'Edit'} variant="secondary" onClick={() => setEditing(editing === group.id ? null : group.id)} />
                {isOther ? null : <Button title="Remove" variant="danger" onClick={() => change(removeGroup(settings, group.id))} />}
              </div>
            ) : null}
            {editing === group.id ? (
              <div class="stack">
                <TextRow label="Name" value={group.name} onChangeText={(name) => editGroup(group.id, { name })} />
                <IconPicker value={group.icon} onChange={(icon) => editGroup(group.id, { icon: icon as typeof group.icon })} />
              </div>
            ) : null}
            {members.length === 0 ? <ThemedText themeColor="textSecondary">No apps; the watch hides an empty group.</ThemedText> : null}
            {members.map((member, index) => (
              <div key={member.id} class="entry">
                <button
                  type="button"
                  class="text-button text-default"
                  style={{ padding: 0, color: chosenApp === member.id ? theme.accent : theme.text }}
                  onClick={() => setChosenApp(chosenApp === member.id ? null : member.id)}>
                  {member.enabled ? member.name : `${member.name} (hidden)`}
                </button>
                {chosenApp === member.id ? (
                  <>
                    {custom ? (
                      <div class="actions">
                        <Button title="Earlier" variant="secondary" disabled={index === 0} onClick={() => change(placeApp(settings, apps, member.id, group.id, index - 1))} />
                        <Button title="Later" variant="secondary" disabled={index === members.length - 1} onClick={() => change(placeApp(settings, apps, member.id, group.id, index + 1))} />
                      </div>
                    ) : null}
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
              </div>
            ))}
          </Card>
        );
      })}

      <Card title="New group">
        <TextRow label="Name" placeholder="Fitness" value={newName} onChangeText={setNewName} />
        <Button title="Add group" disabled={!newName.trim() || settings.groups.length >= MOST_GROUPS} onClick={addGroup} />
      </Card>
    </Screen>
  );
}
