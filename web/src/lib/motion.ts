// Motion in the catalog: a menu, a dialog or a bar coming in, briefly, and
// not at all for someone who has asked the system for less of it. Nothing
// goes out with motion, and the books themselves stand still: the catalog
// must never wait for an animation.

import { fade as svelteFade, fly as svelteFly, scale as svelteScale } from 'svelte/transition';

export function reducedMotion(): boolean {
	if (typeof document !== 'undefined' && document.documentElement.dataset.motion === 'off') return true;
	return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** `ms`, or nothing when motion is unwanted or `still` says so. */
export function lasting(ms: number, still = false): number {
	return still || reducedMotion() ? 0 : ms;
}

type Still = { still?: boolean };

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
