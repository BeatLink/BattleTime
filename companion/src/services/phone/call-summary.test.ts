import { describe, expect, it } from '@jest/globals';

import { CallSummary } from './call-summary';

describe('CallSummary', () => {
  it('reports a call that rang out as missed', () => {
    const summary = new CallSummary();
    expect(summary.noteCall({ type: 'call', state: 'incoming', name: 'Sam', number: '555' })).toBeNull();
    expect(summary.noteCall({ type: 'call', state: 'end', name: 'Sam', number: '555' })).toEqual({
      t: 'notify',
      id: -1,
      src: 'Phone',
      title: 'Sam',
      body: 'Missed call',
    });
  });

  it('reports a call answered on the phone or the watch as accepted', () => {
    const summary = new CallSummary();
    summary.noteCall({ type: 'call', state: 'incoming', number: '555' });
    summary.noteWatchAnswer('IGNORE');
    summary.noteCall({ type: 'call', state: 'start', number: '555' });
    expect(summary.noteCall({ type: 'call', state: 'end' })).toMatchObject({ title: '555', body: 'Accepted call' });
  });

  it('reports a call rejected or muted on the watch as ignored', () => {
    const summary = new CallSummary();
    for (const command of ['REJECT', 'IGNORE'] as const) {
      summary.noteCall({ type: 'call', state: 'incoming' });
      summary.noteWatchAnswer('ACCEPT');
      summary.noteWatchAnswer(command);
      expect(summary.noteCall({ type: 'call', state: 'end' })).toMatchObject({
        title: 'Unknown caller',
        body: 'Ignored call',
      });
    }
  });

  it('gives each summary its own id below the phone notifications', () => {
    const summary = new CallSummary();
    const ids = [1, 2].map(() => {
      summary.noteCall({ type: 'call', state: 'incoming', name: 'Sam' });
      return summary.noteCall({ type: 'call', state: 'end' })?.id;
    });
    expect(ids).toEqual([-1, -2]);
  });

  it('leaves out outgoing calls and stray ends and answers', () => {
    const summary = new CallSummary();
    summary.noteWatchAnswer('REJECT');
    expect(summary.noteCall({ type: 'call', state: 'end' })).toBeNull();
    summary.noteCall({ type: 'call', state: 'incoming', name: 'Sam' });
    summary.noteCall({ type: 'call', state: 'outgoing', name: 'Ann' });
    summary.noteCall({ type: 'call', state: 'start', name: 'Ann' });
    expect(summary.noteCall({ type: 'call', state: 'end', name: 'Ann' })).toBeNull();
  });
});
