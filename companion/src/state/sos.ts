// The SOS settings and what the daemon reports about an SOS, as core/src/daemon/sos.rs reads and writes them.

// ============================================================================
// Settings
// ============================================================================

export type SosContact = { name: string; number: string };

// Whom the SOS calls: the contacts in turn, or the emergency number through the dialer.
export type SosCall = 'contacts' | 'emergency';

export const SOS_CALL_CHOICES: { value: SosCall; label: string }[] = [
  { value: 'contacts', label: 'Emergency contacts' },
  { value: 'emergency', label: 'Emergency number' },
];

export const DEFAULT_EMERGENCY_NUMBER = '911';

// Stored contacts, keeping only well-formed ones so a bad entry never reaches the phone.
export function readSosContacts(raw: unknown): SosContact[] {
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw
    .filter((entry): entry is { name?: unknown; number: string } => typeof entry?.number === 'string')
    .map((entry) => ({ name: typeof entry.name === 'string' ? entry.name : '', number: entry.number }));
}

// A contact from the screen's two fields, or an error saying what is wrong with them.
export function contactFromFields(name: string, number: string): SosContact {
  const digits = number.replace(/[^0-9]/g, '');
  if (digits.length < 3) {
    throw new Error('Enter a phone number for the contact.');
  }
  if (!/^\+?[0-9 ()\-.]+$/.test(number.trim())) {
    throw new Error('A phone number holds only digits, spaces, brackets, dashes and a leading +.');
  }
  return { name: name.trim(), number: number.trim() };
}

export function contactName(contact: SosContact): string {
  return contact.name || contact.number;
}

// ============================================================================
// What the daemon reports
// ============================================================================

export type SosLogEntry = { at: number; ok: boolean; message: string };

export type SosStatus = { active: boolean; startedAt: number | null; log: SosLogEntry[] };

export const IDLE_SOS: SosStatus = { active: false, startedAt: null, log: [] };
