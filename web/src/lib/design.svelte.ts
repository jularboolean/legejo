// The look of the catalog: light, dark, or Zamenhof, a brown and green one
// after old Esperanto print. "auto" follows the system between light and
// dark, which is what Legejo did before there was a choice. The choice is
// kept on the device and put on <html> as `data-design`; app.css holds the
// colours, and app.html sets the attribute before the first paint.

export type Design = 'auto' | 'light' | 'dark' | 'zamenhof';

export const DESIGNS: Design[] = ['auto', 'light', 'dark', 'zamenhof'];
const KEY = 'legejo.design';

/** The colour of the page behind everything, for the browser's own chrome. */
export const PAGE_COLOR: Record<Exclude<Design, 'auto'>, string> = {
	light: '#f3f5f5',
	dark: '#15181a',
	zamenhof: '#ece0c8'
};

function load(): Design {
	try {
		const saved = localStorage.getItem(KEY);
		if (DESIGNS.includes(saved as Design)) return saved as Design;
	} catch {
		// No storage: the system decides.
	}
	return 'auto';
}

let current = $state<Design>(load());

export function design(): Design {
	return current;
}

export function setDesign(next: Design) {
	current = next;
	try {
		if (next === 'auto') localStorage.removeItem(KEY);
		else localStorage.setItem(KEY, next);
	} catch {
		// The choice lasts as long as the page.
	}
	if (next === 'auto') delete document.documentElement.dataset.design;
	else document.documentElement.dataset.design = next;
}

// Motion: on unless turned off here (or by the system, which motion.ts and
// the styles ask about themselves). Off is `data-motion="off"` on <html>.
const MOTION_KEY = 'legejo.motion';

function loadMotion(): boolean {
	try {
		return localStorage.getItem(MOTION_KEY) !== 'off';
	} catch {
		return true;
	}
}

let moving = $state(loadMotion());

export function motion(): boolean {
	return moving;
}

export function setMotion(on: boolean) {
	moving = on;
	try {
		if (on) localStorage.removeItem(MOTION_KEY);
		else localStorage.setItem(MOTION_KEY, 'off');
	} catch {
		// The choice lasts as long as the page.
	}
	if (on) delete document.documentElement.dataset.motion;
	else document.documentElement.dataset.motion = 'off';
}
