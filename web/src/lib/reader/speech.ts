// Reading aloud with the browser's own speech synthesis. The engine supplies
// the sentences of the chapter on screen and marks the one being read; this
// module speaks them in order and moves on through the book.

import { primaryLanguage } from '#lib/library';
import type { ReaderEngine } from './types';

export type SpeechState = 'off' | 'playing' | 'paused';

/** Per-device preferences (persisted in localStorage). */
export type SpeechPrefs = {
	/** Speed, 1 is the voice's normal pace. */
	rate: number;
	/** Chosen voice per language ("sv" → voiceURI). */
	voices: Record<string, string>;
};

export const RATE = { min: 0.6, max: 2.4, step: 0.2 } as const;
const KEY = 'legejo.reader.speech';
/** Longer stretches are cut at a clause: some browsers stop speaking mid-utterance. */
const CHUNK_CHARS = 220;
/** Chapters without text (pictures, blank pages) passed over in a row before giving up. */
const EMPTY_CHAPTERS = 12;

export function speechSupported(): boolean {
	return typeof window !== 'undefined' && 'speechSynthesis' in window && 'SpeechSynthesisUtterance' in window;
}

export function loadSpeechPrefs(): SpeechPrefs {
	const prefs: SpeechPrefs = { rate: 1, voices: {} };
	try {
		const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<SpeechPrefs> | null;
		if (typeof raw?.rate === 'number' && Number.isFinite(raw.rate)) {
			prefs.rate = Math.min(RATE.max, Math.max(RATE.min, raw.rate));
		}
		if (raw?.voices && typeof raw.voices === 'object') {
			for (const [lang, uri] of Object.entries(raw.voices)) if (typeof uri === 'string') prefs.voices[lang] = uri;
		}
	} catch {
		// Unreadable: the defaults.
	}
	return prefs;
}

export function saveSpeechPrefs(prefs: SpeechPrefs): void {
	try {
		localStorage.setItem(KEY, JSON.stringify(prefs));
	} catch {
		// Private mode or full storage: the choice lasts for this visit.
	}
}

/** The device's voices for a language, the most suitable first. */
export function voicesFor(language: string | null): SpeechSynthesisVoice[] {
	if (!speechSupported()) return [];
	const all = speechSynthesis.getVoices();
	if (!language) return all;
	const rank = (v: SpeechSynthesisVoice) => (v.default ? 0 : v.localService ? 1 : 2);
	return all.filter((v) => primaryLanguage(v.lang) === language).sort((a, b) => rank(a) - rank(b));
}

/** Cut a long sentence at clause boundaries, so that no part is too long to speak. */
export function chunks(sentence: string, limit = CHUNK_CHARS): string[] {
	if (sentence.length <= limit) return [sentence];
	const out: string[] = [];
	let rest = sentence;
	while (rest.length > limit) {
		const head = rest.slice(0, limit);
		// The last clause break in reach, else the last space.
		let cut = Math.max(head.lastIndexOf(', '), head.lastIndexOf('; '), head.lastIndexOf(': '), head.lastIndexOf(' – '), head.lastIndexOf(' — '));
		if (cut < limit / 3) cut = head.lastIndexOf(' ');
		if (cut <= 0) cut = limit - 1;
		out.push(rest.slice(0, cut + 1).trim());
		rest = rest.slice(cut + 1);
	}
	if (rest.trim()) out.push(rest.trim());
	return out;
}

export type SpeechOptions = {
	engine: ReaderEngine;
	prefs: () => SpeechPrefs;
	onState: (state: SpeechState) => void;
	/** The language being read ("sv"), and whether the device has a voice for it. */
	onLanguage: (language: string | null, hasVoice: boolean) => void;
	/** The last sentence of the book has been read. */
	onFinished: () => void;
	/** The browser could not speak. */
	onError: () => void;
};

export type Speech = {
	readonly state: SpeechState;
	play(): void;
	pause(): void;
	toggle(): void;
	stop(): void;
	/** One sentence forward or back. */
	step(direction: 1 | -1): void;
	/** The reader has moved in the book: go on from what is on screen now. */
	resync(): void;
	/** Speed or voice changed: say the current sentence again with it. */
	refresh(): void;
};

export function createSpeech(options: SpeechOptions): Speech {
	const { engine } = options;
	let state: SpeechState = 'off';
	let sentences: string[] = [];
	let index = 0;
	let chapter = -1;
	let language: string | null = null;
	/** Bumped whenever what is being said is abandoned; late callbacks then do nothing. */
	let turn = 0;
	/** Kept referenced while it speaks: a collected utterance never reports its end. */
	let utterance: SpeechSynthesisUtterance | null = null;
	/** The speech itself is turning the page, not the reader. */
	let moving = false;

	function setState(next: SpeechState) {
		if (state === next) return;
		state = next;
		options.onState(next);
	}

	function hush() {
		turn++;
		utterance = null;
		speechSynthesis.cancel();
	}

	function voice(): SpeechSynthesisVoice | null {
		const candidates = voicesFor(language);
		const chosen = language ? options.prefs().voices[language] : undefined;
		return candidates.find((v) => v.voiceURI === chosen) ?? candidates[0] ?? null;
	}

	/** Fetch the sentences of the chapter on screen. False when nothing is shown. */
	function load(): boolean {
		const text = engine.speechText();
		if (!text) return false;
		sentences = text.sentences;
		index = text.first;
		chapter = text.chapter;
		language = primaryLanguage(text.language);
		options.onLanguage(language, voicesFor(language).length > 0);
		return true;
	}

	/** Move to the next chapter that has text. False at the end of the book. */
	async function advance(my: number): Promise<boolean> {
		for (let i = 0; i < EMPTY_CHAPTERS; i++) {
			const before = chapter;
			moving = true;
			try {
				await engine.nextChapter();
			} finally {
				moving = false;
			}
			if (my !== turn) return true;
			// Still the same chapter: that was the last one.
			if (!load() || chapter === before) return false;
			index = 0;
			if (sentences.length > 0) return true;
		}
		return false;
	}

	async function speak() {
		const my = ++turn;
		speechSynthesis.cancel();
		if (index >= sentences.length) {
			const more = await advance(my);
			if (my !== turn) return;
			if (!more) {
				stop();
				options.onFinished();
				return;
			}
		}
		moving = true;
		let shown: boolean;
		try {
			shown = await engine.speechShow(index);
		} finally {
			moving = false;
		}
		if (my !== turn) return;
		if (!shown) {
			// The chapter was laid out again (a resize, a new text size).
			if (!load()) return stop();
			return void speak();
		}
		const parts = chunks(sentences[index]);
		const say = (part: number) => {
			if (my !== turn) return;
			if (part >= parts.length) {
				index++;
				return void speak();
			}
			const u = new SpeechSynthesisUtterance(parts[part]);
			const v = voice();
			if (v) u.voice = v;
			u.lang = v?.lang ?? language ?? '';
			u.rate = options.prefs().rate;
			u.onend = () => say(part + 1);
			u.onerror = (event) => {
				if (my !== turn || event.error === 'interrupted' || event.error === 'canceled') return;
				stop();
				options.onError();
			};
			utterance = u;
			speechSynthesis.speak(u);
		};
		// Right after a cancel some browsers drop the next utterance.
		setTimeout(() => say(0), 60);
	}

	function stop() {
		if (state === 'off') return;
		hush();
		setState('off');
		engine.speechShow(null).catch(() => {});
	}

	const speech: Speech = {
		get state() {
			return state;
		},
		play() {
			if (state === 'playing') return;
			if (state === 'off' && !load()) return;
			setState('playing');
			void speak();
		},
		pause() {
			if (state !== 'playing') return;
			// Cancel rather than pause: a paused synthesizer does not resume
			// everywhere. The sentence is read again from its start.
			hush();
			setState('paused');
		},
		toggle() {
			if (state === 'playing') speech.pause();
			else speech.play();
		},
		stop,
		step(direction) {
			if (state === 'off') return;
			index = Math.max(0, Math.min(sentences.length, index + direction));
			if (state === 'playing') void speak();
			else {
				hush();
				engine.speechShow(index < sentences.length ? index : null).catch(() => {});
			}
		},
		resync() {
			if (state === 'off' || moving) return;
			// Still reading what is on screen: the reader only adjusted the view.
			if (index < sentences.length && engine.speechOnScreen(index)) return;
			hush();
			if (!load()) return stop();
			if (state === 'playing') void speak();
			else engine.speechShow(index < sentences.length ? index : null).catch(() => {});
		},
		refresh() {
			if (state === 'playing') void speak();
		}
	};
	return speech;
}
