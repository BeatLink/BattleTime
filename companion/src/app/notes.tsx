import { useEffect, useRef, useState } from 'react';

import { Card } from '@/components/card';
import { Button } from '@/components/button';
import { ChoiceRow, ErrorNote, SavedTextRow, TextRow } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { LARGEST_NOTE_BYTES, NOTE_FONTS, type NoteFont } from '@/protocol/notes';
import { describeSource, type NoteSource } from '@/services/note-source';
import type { NoteStatus } from '@/state/note-sync';
import { useTask } from '@/state/use-task';

const FONT_CHOICES = NOTE_FONTS.map((font) => ({ value: font, label: font.charAt(0).toUpperCase() + font.slice(1) }));

const SOURCE_CHOICES: { value: NoteSource; label: string }[] = [
  { value: 'typed', label: 'Typed here' },
  { value: 'file', label: 'A file' },
  { value: 'url', label: 'A URL' },
];

// Typing is saved, and so sent, once it pauses for this long.
const TYPING_PAUSE_MS = 1500;

const size = (bytes: number) => (bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} kB`);

const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

// One line on how keeping the watch's note up to date is going.
function describe(status: NoteStatus, connected: boolean): string {
  if (!connected) {
    return 'The note is sent when the watch connects.';
  }
  switch (status.kind) {
    case 'idle':
      return 'Checking the note…';
    case 'sending':
      return `Sending ${size(status.bytes)} to the watch…`;
    case 'synced':
      return `On the watch, ${size(status.bytes)}; checked at ${clock(status.checkedAt)}.`;
    case 'failed':
      return status.message;
  }
}

// The note's text box, which saves what is typed once typing pauses or the box loses focus.
function NoteEditor({ value, onSave }: { value: string; onSave: (text: string) => void }) {
  const [draft, setDraft] = useState(value);
  const saved = useRef(value);
  useEffect(() => {
    // Clearing the note elsewhere empties the box too.
    if (value !== saved.current) {
      saved.current = value;
      setDraft(value);
    }
  }, [value]);
  const save = (text: string) => {
    if (text !== saved.current) {
      saved.current = text;
      onSave(text);
    }
  };
  useEffect(() => {
    const timer = setTimeout(() => save(draft), TYPING_PAUSE_MS);
    return () => clearTimeout(timer);
    // Only a change to the draft restarts the pause.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);
  return (
    <TextRow
      label="Note"
      detail="Sent to the watch a moment after you stop typing."
      placeholder="Shopping list, directions, anything…"
      multiline
      autoCapitalize="sentences"
      autoCorrect
      style={{ minHeight: 160, textAlignVertical: 'top' }}
      value={draft}
      onChangeText={setDraft}
      onBlur={() => save(draft)}
    />
  );
}

export default function NotesScreen() {
  const { settings, updateSettings, watchReady, capabilities, noteFiles, noteStatus, syncNote, forgetNote } =
    useWatch();
  const task = useTask();
  const connected = watchReady && capabilities.files;
  const source = settings.noteSource;
  const chosen = describeSource(settings);

  const choose = () =>
    task.run('Choosing a file', async () => {
      const picked = await noteFiles.pick();
      if (picked) {
        await updateSettings({ noteFile: picked });
      }
    });

  return (
    <Screen title="Notes">
      <Card title="Note">
        <ThemedText type="small" themeColor="textSecondary">
          The watch&apos;s Notes app shows this note, read only. It is sent again whenever it changes, up to{' '}
          {LARGEST_NOTE_BYTES / 1024} kB.
        </ThemedText>
        <ChoiceRow<NoteSource>
          label="Where the note comes from"
          options={SOURCE_CHOICES}
          value={source}
          onChange={(noteSource) => void updateSettings({ noteSource })}
        />

        {source === 'typed' ? (
          <NoteEditor value={settings.noteText} onSave={(noteText) => void updateSettings({ noteText })} />
        ) : null}

        {source === 'file' ? (
          <>
            <ThemedText>{settings.noteFile ? settings.noteFile.name : 'No file chosen'}</ThemedText>
            <Button
              title={settings.noteFile ? 'Choose another file' : 'Choose a text file'}
              disabled={task.busy !== null}
              onPress={() => void choose()}
            />
          </>
        ) : null}

        {source === 'url' ? (
          <SavedTextRow
            // A new key resets the box when the URL is cleared elsewhere.
            key={settings.noteUrl === '' ? 'empty' : 'set'}
            label="URL"
            detail="The app fetches it every minute and sends the text when it changes."
            placeholder="https://example.com/note.txt"
            keyboardType="url"
            value={settings.noteUrl}
            onSave={(noteUrl) => void updateSettings({ noteUrl: noteUrl.trim() })}
          />
        ) : null}

        {chosen ? (
          <ThemedText type="small" themeColor={noteStatus.kind === 'failed' ? 'danger' : 'textSecondary'}>
            {describe(noteStatus, connected)}
          </ThemedText>
        ) : null}
        {chosen && connected ? (
          <Button title="Send now" variant="secondary" disabled={task.busy !== null} onPress={() => void syncNote()} />
        ) : null}
        {chosen ? (
          <Button
            title={connected ? 'Clear and remove it from the watch' : 'Clear it'}
            variant="secondary"
            disabled={task.busy !== null}
            onPress={() => void task.run('Removing the note', forgetNote)}
          />
        ) : null}
      </Card>

      <Card title="Text size">
        <ChoiceRow<NoteFont>
          label="Font on the watch"
          detail="Larger text shows fewer lines at a time."
          options={FONT_CHOICES}
          value={settings.noteFont}
          onChange={(font) => void updateSettings({ noteFont: font })}
        />
      </Card>

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
