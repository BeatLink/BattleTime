/**
 * @jest-environment jsdom
 */
import '@/test-support/jsdom-text-encoding';

import { afterEach, describe, expect, it } from '@jest/globals';
import { render } from 'preact';

import { qrBitmap } from '@/protocol/qr-code';

import { LibraryIcon } from './library-icon';
import { QrPreview } from './qr-preview';

// ============================================================================
// Helpers
// ============================================================================

// Draws a component into a fresh element on the page.
function draw(element: preact.ComponentChild): HTMLElement {
  const host = document.createElement('div');
  document.body.appendChild(host);
  render(element, host);
  return host;
}

afterEach(() => {
  document.body.innerHTML = '';
});

// ============================================================================
// Tests
// ============================================================================

// Preact 11 no longer adds "px" to a number, so a size given as a bare number is lost.
describe('inline sizes', () => {
  it('gives the QR code preview and each dark run a size in pixels', () => {
    const host = draw(<QrPreview bitmap={qrBitmap('https://example.org')} size={120} />);
    const paper = host.querySelector<HTMLElement>('.qr-paper')!;
    const ink = host.querySelector<HTMLElement>('.qr-ink')!;
    expect(paper.style.width).toMatch(/^\d+px$/);
    expect(paper.style.height).toBe(paper.style.width);
    expect(ink.style.width).toMatch(/^\d+px$/);
    expect(ink.style.height).toMatch(/^\d+px$/);
    expect(ink.style.left).toMatch(/^\d+px$/);
  });

  it('gives a library icon its size in pixels', () => {
    const host = draw(<LibraryIcon name="no-such-icon" size={20} />);
    const label = host.querySelector<HTMLElement>('p')!;
    expect(label.style.width).toBe('40px');
  });
});
