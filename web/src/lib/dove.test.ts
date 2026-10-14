import { describe, expect, it } from 'vitest';
import raw from './assets/duva.svg?raw';

// Dove.svelte reads the logo's colours out of the file to draw a filled
// rating dove. When the drawing was redone its fills moved from style to
// attributes, nothing matched, and every filled dove came out black.
describe('the rating dove', () => {
	it('finds the colours of the logo it is drawn from', () => {
		const fills = [...raw.matchAll(/<path\b[^>]*>/g)].map(([tag]) => /fill(?::|=")\s*(#[0-9a-fA-F]{6})/.exec(tag)?.[1]?.toLowerCase());
		expect(fills.every(Boolean)).toBe(true);
		expect(fills).toContain('#5a7075');
	});
});
