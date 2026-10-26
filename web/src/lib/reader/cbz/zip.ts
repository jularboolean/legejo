// Reading the files of a zip archive held in memory, as a comic book archive
// (CBZ) is: the central directory at the end lists every file and where it
// starts, and each is stored as it is or deflated, which the browser's own
// DecompressionStream undoes. A file is unpacked only when it is asked for.

export type ZipEntry = {
	name: string;
	/** 0 stored, 8 deflated. */
	method: number;
	compressedSize: number;
	size: number;
	/** Where the entry's local header starts. */
	offset: number;
};

const EOCD = 0x06054b50;
const EOCD64_LOCATOR = 0x07064b50;
const EOCD64 = 0x06064b50;
const CENTRAL = 0x02014b50;
const LOCAL = 0x04034b50;

export class ZipError extends Error {}

/** The files of an archive, in the order of its directory. */
export function readEntries(data: ArrayBuffer): ZipEntry[] {
	const view = new DataView(data);
	// The end record: 22 bytes and a comment of up to 65535, searched from the end.
	let end = -1;
	for (let i = data.byteLength - 22; i >= Math.max(0, data.byteLength - 22 - 0xffff); i--) {
		if (view.getUint32(i, true) === EOCD) {
			end = i;
			break;
		}
	}
	if (end < 0) throw new ZipError('no end of central directory');
	let count = view.getUint16(end + 10, true);
	let dirOffset = view.getUint32(end + 16, true);
	// ZIP64: the real numbers are in a record the locator before the end record points to.
	if ((count === 0xffff || dirOffset === 0xffffffff) && end >= 20 && view.getUint32(end - 20, true) === EOCD64_LOCATOR) {
		const at = Number(view.getBigUint64(end - 20 + 8, true));
		if (view.getUint32(at, true) !== EOCD64) throw new ZipError('bad zip64 end record');
		count = Number(view.getBigUint64(at + 32, true));
		dirOffset = Number(view.getBigUint64(at + 48, true));
	}
	const decoder = new TextDecoder();
	const latin = new TextDecoder('latin1');
	const entries: ZipEntry[] = [];
	let p = dirOffset;
	for (let i = 0; i < count; i++) {
		if (p + 46 > data.byteLength || view.getUint32(p, true) !== CENTRAL) throw new ZipError('bad central directory');
		const flags = view.getUint16(p + 8, true);
		const method = view.getUint16(p + 10, true);
		let compressedSize = view.getUint32(p + 20, true);
		let size = view.getUint32(p + 24, true);
		const nameLength = view.getUint16(p + 28, true);
		const extraLength = view.getUint16(p + 30, true);
		const commentLength = view.getUint16(p + 32, true);
		let offset = view.getUint32(p + 42, true);
		const nameBytes = new Uint8Array(data, p + 46, nameLength);
		// Bit 11: the name is UTF-8; otherwise code page 437, near enough Latin-1 for names.
		const name = (flags & 0x800 ? decoder : latin).decode(nameBytes);
		// ZIP64 extra field: the 64-bit values, in order, of those that are all ones.
		let e = p + 46 + nameLength;
		const extraEnd = e + extraLength;
		while (e + 4 <= extraEnd) {
			const id = view.getUint16(e, true);
			const length = view.getUint16(e + 2, true);
			if (id === 0x0001) {
				let q = e + 4;
				if (size === 0xffffffff) (size = Number(view.getBigUint64(q, true))), (q += 8);
				if (compressedSize === 0xffffffff) (compressedSize = Number(view.getBigUint64(q, true))), (q += 8);
				if (offset === 0xffffffff) offset = Number(view.getBigUint64(q, true));
			}
			e += 4 + length;
		}
		entries.push({ name, method, compressedSize, size, offset });
		p = extraEnd + commentLength;
	}
	return entries;
}

/** The bytes of a file of the archive, unpacked. */
export async function readEntry(data: ArrayBuffer, entry: ZipEntry): Promise<Uint8Array<ArrayBuffer>> {
	const view = new DataView(data);
	if (view.getUint32(entry.offset, true) !== LOCAL) throw new ZipError('bad local header');
	// The local header has its own name and extra lengths, not always those of the directory.
	const start = entry.offset + 30 + view.getUint16(entry.offset + 26, true) + view.getUint16(entry.offset + 28, true);
	const packed = new Uint8Array(data, start, entry.compressedSize);
	if (entry.method === 0) return packed.slice();
	if (entry.method !== 8) throw new ZipError(`unsupported compression ${entry.method}`);
	const stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
	return new Uint8Array(await new Response(stream).arrayBuffer());
}
