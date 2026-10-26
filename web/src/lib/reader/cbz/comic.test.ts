import { describe, expect, it } from 'vitest';
import { comicContents, encodePage, naturalCompare, pageOf, readsRightToLeft, spreads } from './comic';
import { readEntries, readEntry, ZipError, type ZipEntry } from './zip';

/** A zip archive of the given files, the first ones stored and the rest deflated. */
async function makeZip(files: { name: string; data: Uint8Array<ArrayBuffer>; deflate?: boolean }[]): Promise<ArrayBuffer> {
	const parts: Uint8Array<ArrayBuffer>[] = [];
	const central: Uint8Array<ArrayBuffer>[] = [];
	let offset = 0;
	const encoder = new TextEncoder();
	for (const file of files) {
		const name = encoder.encode(file.name);
		const body = file.deflate
			? new Uint8Array(await new Response(new Blob([file.data]).stream().pipeThrough(new CompressionStream('deflate-raw'))).arrayBuffer())
			: file.data;
		const local = new DataView(new ArrayBuffer(30));
		local.setUint32(0, 0x04034b50, true);
		local.setUint16(6, 0x800, true);
		local.setUint16(8, file.deflate ? 8 : 0, true);
		local.setUint32(18, body.length, true);
		local.setUint32(22, file.data.length, true);
		local.setUint16(26, name.length, true);
		parts.push(new Uint8Array(local.buffer), name, body);
		const dir = new DataView(new ArrayBuffer(46));
		dir.setUint32(0, 0x02014b50, true);
		dir.setUint16(8, 0x800, true);
		dir.setUint16(10, file.deflate ? 8 : 0, true);
		dir.setUint32(20, body.length, true);
		dir.setUint32(24, file.data.length, true);
		dir.setUint16(28, name.length, true);
		dir.setUint32(42, offset, true);
		central.push(new Uint8Array(dir.buffer), name);
		offset += 30 + name.length + body.length;
	}
	const dirSize = central.reduce((n, c) => n + c.length, 0);
	const end = new DataView(new ArrayBuffer(22));
	end.setUint32(0, 0x06054b50, true);
	end.setUint16(8, files.length, true);
	end.setUint16(10, files.length, true);
	end.setUint32(12, dirSize, true);
	end.setUint32(16, offset, true);
	return await new Blob([...parts, ...central, new Uint8Array(end.buffer)]).arrayBuffer();
}

const bytes = (text: string) => new TextEncoder().encode(text);

describe('zip', () => {
	it('lists the files and unpacks stored and deflated ones', async () => {
		const zip = await makeZip([
			{ name: 'åäö/01.jpg', data: bytes('first page') },
			{ name: 'ComicInfo.xml', data: bytes('<Manga>YesAndRightToLeft</Manga>'.repeat(20)), deflate: true }
		]);
		const entries = readEntries(zip);
		expect(entries.map((e) => [e.name, e.method])).toEqual([
			['åäö/01.jpg', 0],
			['ComicInfo.xml', 8]
		]);
		expect(new TextDecoder().decode(await readEntry(zip, entries[0]))).toBe('first page');
		expect(new TextDecoder().decode(await readEntry(zip, entries[1]))).toBe('<Manga>YesAndRightToLeft</Manga>'.repeat(20));
	});

	it('refuses what is not a zip archive', () => {
		expect(() => readEntries(bytes('not a zip at all, not even close').buffer as ArrayBuffer)).toThrow(ZipError);
	});
});

describe('comic', () => {
	const entry = (name: string): ZipEntry => ({ name, method: 0, compressedSize: 0, size: 0, offset: 0 });

	it('orders the pages as they are read and leaves out what is not a page', () => {
		const names = ['p10.jpg', 'p2.png', '__MACOSX/._p1.jpg', '.thumb.jpg', 'p1.jpeg', 'notes.txt', 'ComicInfo.xml', 'extra/'];
		const { pages, info } = comicContents(names.map(entry));
		expect(pages.map((p) => p.entry.name)).toEqual(['p1.jpeg', 'p2.png', 'p10.jpg']);
		expect(pages.map((p) => p.type)).toEqual(['image/jpeg', 'image/png', 'image/jpeg']);
		expect(info?.name).toBe('ComicInfo.xml');
		expect(naturalCompare('Vol 2/9.jpg', 'Vol 10/1.jpg')).toBeLessThan(0);
	});

	it('knows manga from ComicInfo.xml', () => {
		expect(readsRightToLeft('<ComicInfo><Manga>YesAndRightToLeft</Manga></ComicInfo>')).toBe(true);
		expect(readsRightToLeft('<ComicInfo><Manga>Yes</Manga></ComicInfo>')).toBe(false);
	});

	it('keeps a place as cbz:<page>', () => {
		expect(pageOf(encodePage(12))).toBe(12);
		expect(pageOf('pdf:3')).toBeNull();
		expect(pageOf('cbz:0')).toBeNull();
		expect(pageOf(null)).toBeNull();
	});

	it('pairs pages in spreads, the cover and wide pages alone', () => {
		expect(spreads(5, false, () => false)).toEqual([[1], [2], [3], [4], [5]]);
		expect(spreads(6, true, () => false)).toEqual([[1], [2, 3], [4, 5], [6]]);
		expect(spreads(7, true, (n) => n === 4)).toEqual([[1], [2, 3], [4], [5, 6], [7]]);
		expect(spreads(6, true, (n) => n === 3)).toEqual([[1], [2], [3], [4, 5], [6]]);
	});
});
