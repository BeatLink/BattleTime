import { describe, expect, it } from '@jest/globals';

import type { RemoteButton } from '@/protocol/remote';

import { buttonFromForm, EMPTY_FORM, formFromButton, moveItem, parsePairs } from './remote-form';

const BUTTONS: RemoteButton[] = [
  { icon: 'play', label: 'Music', action: { kind: 'builtin', name: 'next' } },
  {
    icon: 'flashlight',
    label: 'Lights',
    action: { kind: 'http', method: 'POST', url: 'https://example.com/lights', headers: { Authorization: 'Bearer x' }, body: 'on' },
  },
  { icon: 'settings', label: '', action: { kind: 'command', command: 'notify-send hi' } },
  { icon: 'navigation', label: 'Map', action: { kind: 'android', intent: { type: 'deeplink', uri: 'geo:0,0' } } },
  {
    icon: 'music',
    label: 'Player',
    action: { kind: 'android', intent: { type: 'activity', packageName: 'p', className: 'p.Main', action: 'GO', extras: { a: '1', b: '2' } } },
  },
  { icon: 'bell', label: 'Ping', action: { kind: 'android', intent: { type: 'broadcast', action: 'p.PING', packageName: 'p' } } },
  { icon: 'bell', label: '', action: { kind: 'android', intent: { type: 'activity', packageName: 'p', className: 'c' } } },
  { icon: 'bell', label: '', action: { kind: 'android', intent: { type: 'broadcast', action: 'A', extras: { k: 'v' } } } },
  { icon: 'bell', label: '', action: { kind: 'http', method: 'GET', url: 'http://x' } },
];

describe('the remote button editor', () => {
  it('turns every kind of button into fields and back unchanged', () => {
    for (const button of BUTTONS) {
      expect(buttonFromForm(formFromButton(button))).toEqual(button);
    }
  });

  it('starts a new button as play/pause', () => {
    expect(buttonFromForm(EMPTY_FORM)).toEqual({ icon: 'play', label: '', action: { kind: 'builtin', name: 'playpause' } });
  });

  it('trims the label and drops a body from a GET', () => {
    expect(buttonFromForm({ ...EMPTY_FORM, label: ' Hi ', kind: 'http', url: 'https://x', body: 'ignored' })).toEqual({
      icon: 'play',
      label: 'Hi',
      action: { kind: 'http', method: 'GET', url: 'https://x' },
    });
  });

  it('says which field is missing or malformed', () => {
    expect(() => buttonFromForm({ ...EMPTY_FORM, icon: 'teapot' })).toThrow('no icon called teapot');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'http' })).toThrow('Give the address to call.');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'http', url: 'ftp://x' })).toThrow('http:// or https://');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'http', url: 'https://x', method: 'PATCH' as 'GET' })).toThrow('GET, POST');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'http', url: 'https://x', headers: 'no colon' })).toThrow('name: value');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'command', command: '  ' })).toThrow('Give the command to run.');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'android' })).toThrow('Give the link to open.');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'android', intentType: 'activity', packageName: 'p' })).toThrow(
      'Give the activity class.',
    );
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'android', intentType: 'broadcast' })).toThrow('Give the broadcast action.');
    expect(() => buttonFromForm({ ...EMPTY_FORM, kind: 'android', intentType: 'broadcast', intentAction: 'A', extras: '=v' })).toThrow(
      'name=value',
    );
  });

  it('reads pairs one a line, splitting at the first separator and skipping blank lines', () => {
    expect(parsePairs('A: b: c\n\n D : e ', ':', 'header')).toEqual({ A: 'b: c', D: 'e' });
  });
});

describe('reordering buttons', () => {
  it('moves a button up or down and keeps it inside the list', () => {
    expect(moveItem(['a', 'b', 'c'], 0, 1)).toEqual(['b', 'a', 'c']);
    expect(moveItem(['a', 'b', 'c'], 2, -1)).toEqual(['a', 'c', 'b']);
    expect(moveItem(['a', 'b', 'c'], 0, -1)).toEqual(['a', 'b', 'c']);
    expect(moveItem(['a', 'b', 'c'], 2, 1)).toEqual(['a', 'b', 'c']);
  });
});
