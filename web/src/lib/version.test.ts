import { describe, expect, it } from 'vitest';
import { isNewer, latestRelease, parseVersion, releaseFrom } from '#lib/version';

describe('versions', () => {
	it('are read with or without the v', () => {
		expect(parseVersion('v1.14.1')).toEqual([1, 14, 1]);
		expect(parseVersion('1.14.1')).toEqual([1, 14, 1]);
		expect(parseVersion('1.14')).toBeNull();
		expect(parseVersion('latest')).toBeNull();
	});

	it('compare number by number, not as text', () => {
		expect(isNewer('1.15.0', '1.14.1')).toBe(true);
		expect(isNewer('1.14.2', '1.14.1')).toBe(true);
		expect(isNewer('2.0.0', '1.99.99')).toBe(true);
		expect(isNewer('1.14.10', '1.14.9')).toBe(true);
		expect(isNewer('1.14.1', '1.14.1')).toBe(false);
		expect(isNewer('1.13.0', '1.14.1')).toBe(false);
		expect(isNewer('nonsense', '1.14.1')).toBe(false);
	});

	it('take the release from what GitHub says, and nothing else', () => {
		expect(releaseFrom({ tag_name: 'v1.15.0', html_url: 'https://github.com/x/releases/tag/v1.15.0' })).toEqual({
			version: '1.15.0',
			url: 'https://github.com/x/releases/tag/v1.15.0'
		});
		expect(releaseFrom({ tag_name: 'nightly' })).toBeNull();
		expect(releaseFrom(null)).toBeNull();
		expect(releaseFrom({ message: 'API rate limit exceeded' })).toBeNull();
	});

	it('asks GitHub and fails loudly when GitHub does not answer with a release', async () => {
		const answer = (status: number, body: unknown) =>
			(async () => ({ ok: status < 400, status, json: async () => body })) as unknown as typeof fetch;
		await expect(latestRelease(answer(200, { tag_name: 'v1.15.0', html_url: 'u' }))).resolves.toEqual({ version: '1.15.0', url: 'u' });
		await expect(latestRelease(answer(403, { message: 'rate limit' }))).rejects.toThrow('403');
		await expect(latestRelease(answer(200, {}))).rejects.toThrow('not a release');
	});
});
