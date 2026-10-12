// Motion in the catalog. It is off until someone turns it on, on the Account
// page (see design.svelte.ts), so what there is may be seen: menus and
// dialogs come and go, new books fade in, and one page gives way to the
// next. Everything asks here whether to move.

import { fade as svelteFade, fly as svelteFly, scale as svelteScale } from 'svelte/transition';

/** True unless motion has been turned on. */
export function reducedMotion(): boolean {
	return typeof document === 'undefined' || document.documentElement.dataset.motion !== 'on';
}

/** `ms`, or nothing when motion is unwanted or `still` says so. */
export function lasting(ms: number, still = false): number {
	return still || reducedMotion() ? 0 : ms;
}

type Still = { still?: boolean };

export function fade(node: Element, params: Still & { duration?: number } = {}) {
	return svelteFade(node, { duration: lasting(params.duration ?? 200, params.still) });
}

/** Comes in from `y` pixels away, as a menu from its button or a bar from the edge. */
export function rise(node: Element, params: Still & { y?: number; duration?: number } = {}) {
	return svelteFly(node, { y: params.y ?? 10, duration: lasting(params.duration ?? 220, params.still) });
}

/** A dialog or a chip growing into place. */
export function pop(node: Element, params: Still & { start?: number; duration?: number } = {}) {
	return svelteScale(node, { start: params.start ?? 0.92, duration: lasting(params.duration ?? 200, params.still) });
}
