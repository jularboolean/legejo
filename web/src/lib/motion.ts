// Motion in the catalog: short, and none at all for someone who has asked
// the system for less of it. Every transition and animation takes its
// duration from here.

import { flip as svelteFlip } from 'svelte/animate';
import { fade as svelteFade, fly as svelteFly, scale as svelteScale } from 'svelte/transition';

/** Above this many items a list rearranges at once: moving them all costs more than it gives. */
export const MOVING_LIMIT = 300;

export function reducedMotion(): boolean {
	return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** `ms`, or nothing when motion is unwanted or `still` says so. */
export function lasting(ms: number, still = false): number {
	return still || reducedMotion() ? 0 : ms;
}

type Still = { still?: boolean };

/** Items of a keyed list glide to their new places. */
export function flip(node: Element, fromTo: { from: DOMRect; to: DOMRect }, params: Still = {}) {
	return svelteFlip(node, fromTo, { duration: lasting(220, params.still) });
}

export function fade(node: Element, params: Still & { duration?: number } = {}) {
	return svelteFade(node, { duration: lasting(params.duration ?? 140, params.still) });
}

/** Comes in from `y` pixels away, as a menu from its button or a bar from the edge. */
export function rise(node: Element, params: Still & { y?: number; duration?: number } = {}) {
	return svelteFly(node, { y: params.y ?? 6, duration: lasting(params.duration ?? 140, params.still) });
}

/** A dialog or a chip growing into place. */
export function pop(node: Element, params: Still & { start?: number; duration?: number } = {}) {
	return svelteScale(node, { start: params.start ?? 0.96, duration: lasting(params.duration ?? 140, params.still) });
}
