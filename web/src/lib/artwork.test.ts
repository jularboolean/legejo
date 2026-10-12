import { describe, expect, it } from 'vitest';
import pair from './assets/legejo.svg?raw';
import starting from '../../static/pigeons.svg?raw';
import federated from '../../../server/src/fed/legejo.svg?raw';
import cover from '../../../cookbook/pigeons.svg?raw';

// The pair of pigeons is kept in one place and copied to where a build
// cannot reach it from: the page shown while the app starts, the pages the
// server writes for federated shelves, and the manual's cover. A copy that
// was forgotten when the artwork changed shows the old pigeons there.
describe('the pigeons', () => {
	it('are the same wherever they are copied to', () => {
		expect(starting).toBe(pair);
		expect(federated).toBe(pair);
		expect(cover).toBe(pair);
	});
});
