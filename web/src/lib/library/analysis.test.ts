import { describe, expect, it } from 'vitest';
import { curvePath, onCircle, ownLevel, sortLines, type Line } from './analysis';

describe('a book’s analysis', () => {
	it('reads an index where higher is easier as a level', () => {
		expect([95, 80, 70, 55, 40, 10].map(ownLevel)).toEqual([1, 1, 2, 3, 4, 5]);
	});

	it('draws the curve inside its box, the highest value at the top', () => {
		const { d, min, max } = curvePath([30, 40, 35], 100, 50);
		expect([min, max]).toEqual([30, 40]);
		expect(d).toBe('M0.0,45.0 L50.0,5.0 L100.0,25.0');
		expect(curvePath([7], 100, 50).d).toBe('');
		// Level values do not divide by zero.
		expect(curvePath([5, 5], 100, 50).d).toBe('M0.0,45.0 L100.0,45.0');
	});

	it('places names on a circle from the top', () => {
		const top = onCircle(0, 4, 100, 100, 50);
		expect([Math.round(top.x), Math.round(top.y)]).toEqual([100, 50]);
		const right = onCircle(1, 4, 100, 100, 50);
		expect([Math.round(right.x), Math.round(right.y)]).toEqual([150, 100]);
	});

	it('orders the lines by place, by the word after and by the word before', () => {
		const lines: Line[] = [
			{ left: 'vio al', hit: 'perro', right: 'negro en la calle', at: 0.1 },
			{ left: '—¿Y el', hit: 'perro', right: '—dijo ella', at: 0.5 },
			{ left: 'con un', hit: 'perro', right: 'blanco', at: 0.3 }
		];
		const at = (order: 'at' | 'right' | 'left') => sortLines(lines, order, 'es').map((l) => l.at);
		expect(at('at')).toEqual([0.1, 0.3, 0.5]);
		expect(at('right')).toEqual([0.3, 0.5, 0.1]);
		// "al", "el", "un": the word next to the hit, punctuation aside.
		expect(at('left')).toEqual([0.1, 0.5, 0.3]);
		expect(lines[0].at).toBe(0.1);
	});
});
