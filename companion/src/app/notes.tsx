import { Card } from '@/components/card';
import { Button } from '@/components/button';
import { ChoiceRow, ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { LARGEST_NOTE_BYTES, NOTE_FONTS, type NoteFont } from '@/protocol/notes';
import type { NoteStatus } from '@/state/note-sync';
import { useTask } from '@/state/use-task';

const FONT_CHOICES = NOTE_FONTS.map((font) => ({ value: font, label: font.charAt(0).toUpperCase() + font.slice(1) }));

const size = (bytes: number) => (bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} kB`);

const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

// One line on how keeping the watch's note up to date is going.
function describe(status: NoteStatus, connected: boolean): string {
  if (!connected) {
    return 'The note is sent when the watch connects.';
  }
  switch (status.kind) {
    case 'idle':
      return 'Checking the file…';
    case 'sending':
      return `Sending ${size(status.bytes)} to the watch…`;
    case 'synced':
      return `On the watch, ${size(status.bytes)}; checked at ${clock(status.checkedAt)}.`;
    case 'failed':
      return status.message;
  }
}

export default function NotesScreen() {
  const { settings, updateSettings, watchReady, capabilities, noteFiles, noteStatus, syncNote, forgetNote } =
    useWatch();
  const task = useTask();
  const connected = watchReady && capabilities.files;
  const file = settings.noteFile;

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
          The watch&apos;s Notes app shows a text file of your choice, read only. It is sent again whenever it
          changes, up to {LARGEST_NOTE_BYTES / 1024} kB.
        </ThemedText>
        <ThemedText>{file ? file.name : 'No file chosen'}</ThemedText>
        {file ? (
          <ThemedText type="small" themeColor={noteStatus.kind === 'failed' ? 'danger' : 'textSecondary'}>
            {describe(noteStatus, connected)}
          </ThemedText>
        ) : null}
        <Button
          title={file ? 'Choose another file' : 'Choose a text file'}
          disabled={task.busy !== null}
          onPress={() => void choose()}
        />
        {file && connected ? (
          <Button title="Send now" variant="secondary" disabled={task.busy !== null} onPress={() => void syncNote()} />
        ) : null}
        {file ? (
          <Button
            title={connected ? 'Stop and remove it from the watch' : 'Stop sending it'}
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
