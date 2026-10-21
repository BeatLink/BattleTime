import { useCallback, useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote, SavedTextRow, TextRow } from '@/components/fields';
import { ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { phoneBridge } from '@/services/phone';
import { NO_SOS_PERMISSIONS, type SosPermissions } from '@/services/phone/types';
import { moveItem } from '@/state/remote-form';
import { contactFromFields, contactName, SOS_CALL_CHOICES } from '@/state/sos';
import { useTask } from '@/state/use-task';

// ============================================================================
// Permissions
// ============================================================================

const PERMISSION_NAMES: { key: keyof SosPermissions; label: string; detail: string }[] = [
  { key: 'calls', label: 'Calls', detail: 'To call your emergency contacts.' },
  { key: 'texts', label: 'Texts', detail: 'To text your contacts where you are.' },
  { key: 'camera', label: 'Camera', detail: 'To record video.' },
  { key: 'microphone', label: 'Microphone', detail: 'To record sound.' },
  { key: 'overlay', label: 'Display over other apps', detail: 'So the SOS screen and calls can start while the app is closed.' },
];

const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

// What Android lets the SOS do, read again whenever the window comes back from Android's settings.
function useSosPermissions(): [SosPermissions, () => Promise<void>] {
  const [allowed, setAllowed] = useState<SosPermissions>(NO_SOS_PERMISSIONS);
  const refresh = useCallback(async () => {
    setAllowed(await phoneBridge.sosPermissions().catch(() => NO_SOS_PERMISSIONS));
  }, []);
  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [refresh]);
  return [allowed, refresh];
}

function PermissionsCard() {
  const [allowed, refresh] = useSosPermissions();
  const task = useTask();
  const missing = PERMISSION_NAMES.filter(({ key }) => !allowed[key]);
  return (
    <Card title="Permissions">
      {missing.length === 0 ? <ThemedText type="small">Everything the SOS needs is allowed.</ThemedText> : null}
      {missing.map(({ key, label, detail }) => (
        <ThemedText key={key} type="small" themeColor="danger">
          {`Not allowed: ${label}. ${detail}`}
        </ThemedText>
      ))}
      <div class="actions">
        {missing.some(({ key }) => key !== 'overlay') ? (
          <Button
            title="Allow calls, texts and recording"
            onClick={() =>
              void task.run('Asking for permissions', async () => {
                await phoneBridge.requestSosPermissions();
                await refresh();
              })
            }
          />
        ) : null}
        {!allowed.overlay ? (
          <Button
            title="Allow display over other apps"
            variant="secondary"
            onClick={() => void task.run('Opening Android settings', () => phoneBridge.openOverlaySettings())}
          />
        ) : null}
      </div>
      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Card>
  );
}

// ============================================================================
// The screen
// ============================================================================

export default function SosScreen() {
  const { settings, updateSettings, sosStatus, stopSos } = useWatch();
  const task = useTask();
  const contacts = settings.sosContacts;
  const [name, setName] = useState('');
  const [number, setNumber] = useState('');
  const [formError, setFormError] = useState<string | null>(null);
  const isPhone = phoneBridge.kind === 'phone';

  const addContact = () => {
    try {
      void updateSettings({ sosContacts: [...contacts, contactFromFields(name, number)] });
      setName('');
      setNumber('');
      setFormError(null);
    } catch (error) {
      setFormError((error as Error).message);
    }
  };
  const remove = (index: number) => void updateSettings({ sosContacts: contacts.filter((_contact, at) => at !== index) });
  const move = (index: number, step: number) => void updateSettings({ sosContacts: moveItem(contacts, index, step) });

  return (
    <Screen title="SOS">
      {!isPhone ? (
        <Card>
          <ThemedText themeColor="danger">
            The SOS runs only in the Android app, which can call, text and record. Settings made here are kept.
          </ThemedText>
        </Card>
      ) : null}

      <Card title={sosStatus.active ? 'SOS on' : 'Status'}>
        {sosStatus.active ? (
          <>
            <ThemedText themeColor="danger">
              {`The watch sent an SOS at ${clock(sosStatus.startedAt ?? Date.now())}.`}
            </ThemedText>
            <Button title="Stop SOS" variant="danger" onClick={() => void task.run('Stopping the SOS', stopSos)} />
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Hold the button in the watch&apos;s SOS app for 5 seconds to send one.
          </ThemedText>
        )}
        {sosStatus.log.map((entry, index) => (
          <ThemedText key={`${entry.at}-${index}`} type="small" themeColor={entry.ok ? 'text' : 'danger'}>
            {`${clock(entry.at)} ${entry.message}`}
          </ThemedText>
        ))}
        <ErrorNote error={task.error} onDismiss={task.clearError} />
      </Card>

      <Card title="Who to call">
        <ChoiceRow
          label="Call"
          options={SOS_CALL_CHOICES}
          value={settings.sosCall}
          onChange={(sosCall) => void updateSettings({ sosCall })}
        />
        {settings.sosCall === 'emergency' ? (
          <>
            <SavedTextRow
              label="Emergency number"
              inputMode="numeric"
              value={settings.sosEmergencyNumber}
              onSave={(sosEmergencyNumber) => void updateSettings({ sosEmergencyNumber: sosEmergencyNumber.trim() })}
            />
            <ThemedText type="small" themeColor="textSecondary">
              Android lets no app call an emergency number itself, so the dialer opens with it typed in and someone
              must tap Call on the phone.
            </ThemedText>
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Each contact is called in turn. A call that ends within 30 seconds counts as unanswered and the next
            contact is called; the phone cannot tell an answered call from voicemail.
          </ThemedText>
        )}
      </Card>

      <Card title="Emergency contacts">
        {contacts.length === 0 ? <ThemedText>No contacts yet.</ThemedText> : null}
        {contacts.map((contact, index) => (
          <div key={`${contact.number}-${index}`} class="entry">
            <ThemedText>{`${index + 1}. ${contactName(contact)}`}</ThemedText>
            {contact.name ? (
              <ThemedText type="small" themeColor="textSecondary">
                {contact.number}
              </ThemedText>
            ) : null}
            <div class="actions">
              <Button title="Up" variant="secondary" disabled={index === 0} onClick={() => move(index, -1)} />
              <Button title="Down" variant="secondary" disabled={index === contacts.length - 1} onClick={() => move(index, 1)} />
              <Button title="Remove" variant="danger" onClick={() => remove(index)} />
            </div>
          </div>
        ))}
        <TextRow label="Name" autoCapitalize="words" value={name} onChangeText={setName} />
        <TextRow label="Phone number" inputMode="numeric" placeholder="+1 555 0100" value={number} onChangeText={setNumber} onSubmit={addContact} />
        <ErrorNote error={formError} />
        <Button title="Add contact" onClick={addContact} />
      </Card>

      <Card title="Also">
        <ToggleRow
          label="Text my location"
          detail="Texts each contact a map link to where the phone is."
          value={settings.sosTextLocation}
          onValueChange={(sosTextLocation) => void updateSettings({ sosTextLocation })}
        />
        <ToggleRow
          label="Record video and sound"
          detail="Both cameras where the phone can, else the back one. During a call the sound is silent."
          value={settings.sosRecord}
          onValueChange={(sosRecord) => void updateSettings({ sosRecord })}
        />
      </Card>

      {isPhone ? <PermissionsCard /> : null}
    </Screen>
  );
}
