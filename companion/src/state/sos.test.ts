import { describe, expect, it } from '@jest/globals';

import { readSettings } from './settings';
import { contactFromFields, contactName, readSosContacts } from './sos';

describe('readSosContacts', () => {
  it('keeps only contacts with a number, filling in a missing name', () => {
    expect(
      readSosContacts([{ name: 'Ann', number: '555' }, { number: '777' }, { name: 'No number' }, null, 'text', { name: 3, number: '9' }]),
    ).toEqual([
      { name: 'Ann', number: '555' },
      { name: '', number: '777' },
      { name: '', number: '9' },
    ]);
    expect(readSosContacts('nonsense')).toEqual([]);
  });
});

describe('contactFromFields', () => {
  it('trims both fields and accepts the usual ways of writing a number', () => {
    expect(contactFromFields(' Ann ', ' +1 (555) 010-0101 ')).toEqual({ name: 'Ann', number: '+1 (555) 010-0101' });
  });

  it('refuses a number that is missing, too short or holds letters', () => {
    expect(() => contactFromFields('Ann', '')).toThrow('Enter a phone number');
    expect(() => contactFromFields('Ann', '12')).toThrow('Enter a phone number');
    expect(() => contactFromFields('Ann', '555 CALL')).toThrow('only digits');
  });
});

describe('contactName', () => {
  it('falls back to the number', () => {
    expect(contactName({ name: '', number: '555' })).toBe('555');
    expect(contactName({ name: 'Ann', number: '555' })).toBe('Ann');
  });
});

describe('the SOS settings', () => {
  it('default to calling the contacts, texting and recording, with 911', () => {
    expect(readSettings({})).toMatchObject({
      sosContacts: [],
      sosCall: 'contacts',
      sosEmergencyNumber: '911',
      sosTextLocation: true,
      sosRecord: true,
    });
  });

  it('read back what was stored and drop what is not understood', () => {
    expect(readSettings({ sosCall: 'emergency', sosContacts: [{ number: '555' }, {}] })).toMatchObject({
      sosCall: 'emergency',
      sosContacts: [{ name: '', number: '555' }],
    });
    expect(readSettings({ sosCall: 'everyone' }).sosCall).toBe('contacts');
  });
});
