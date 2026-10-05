import type { FederableStatus, NotFederable } from '#lib/license';

export type Book = {
	id: number;
	uuid: string;
	title: string;
	author: string | null;
	language: string | null;
	description: string | null;
	publisher: string | null;
	/** This edition's date: "YYYY" or "YYYY-MM-DD". */
	published: string | null;
	/** The year the work first appeared. */
	first_published: number | null;
	category: string | null;
	identifier: string | null;
	isbn: string | null;
	libris_id: string | null;
	file_size: number;
	has_cover: boolean;
	created_at: string;
	updated_at: string | null;
	/** Reading progress 0–1 (web or Kobo, latest wins); absent when the book is unread. */
	progress_percent?: number;
	/** For imported copies: uuid of the original book. */
	source_uuid?: string;
	series: string | null;
	series_index: number | null;
	/** The owner's rating, 1–5. */
	rating: number | null;
	/** Licence facts (lib/license.ts). */
	license: string | null;
	license_source_url: string | null;
	author_death_year: number | null;
	cover_is_free: boolean;
	/** For books fetched from another instance: the remote object IRI. */
	fed_source?: string | null;
	/** On the owner's want-to-read list, and since when. */
	want_to_read: boolean;
	wanted_at: string | null;
	/** When the user last read it (web or Kobo); library listing only. */
	last_read_at?: string | null;
};

/** A book in the library listing: the shelves it sits on come along. */
export type LibraryBook = Book & { shelf_ids: number[] };

export type ShelfRef = { id: number; name: string };

export type BookDetail = Book & {
	/** Removed from the owner's Kobo on the device; left out of the sync. */
	kobo_removed: boolean;
	shelves: ShelfRef[];
	tags: string[];
	/** Whether the book could go on a federated shelf, and if not, why. */
	federable: FederableStatus;
};

/** Who sees a shelf: the owner, chosen users, every user here, or the fediverse. */
export type Visibility = 'private' | 'restricted' | 'instance' | 'federated';

/** A user a restricted shelf is shared with. */
export type ShelfMember = { id: number; username: string; has_avatar: boolean };

export type Shelf = {
	id: number;
	name: string;
	visibility: Visibility;
	/** visibility !== 'private'; kept by the server for older clients. */
	is_public: boolean;
	description: string | null;
	has_cover: boolean;
	book_count: number;
	/** The ActivityPub slug; set the first time the shelf federates, never changes after. */
	ap_slug: string | null;
	/** "@slug@host", only while visibility is 'federated'. */
	handle: string | null;
	/** The server's proposal for ap_slug, from "<user>-<shelf>". */
	suggested_slug: string;
	followers: number;
	/** Who a restricted shelf is shared with; empty in the other modes. */
	members: ShelfMember[];
};

export type ShelfDetail = Shelf & { books: Book[] };

export type LibrisCandidate = {
	libris_id: string | null;
	url: string | null;
	title: string | null;
	creator: string | null;
	publisher: string | null;
	date: string | null;
	language: string | null;
	isbn: string[];
};

export type PublicShelf = {
	id: number;
	name: string;
	description: string | null;
	has_cover: boolean;
	book_count: number;
	owner: string;
	owner_id: number;
	owner_has_avatar: boolean;
	/** Shared with the viewer by name rather than with everyone. */
	restricted: boolean;
};

export type PublicBook = Book & { owned: boolean };

export type PublicShelfDetail = PublicShelf & { books: PublicBook[] };

export type PublicHit = Book & {
	owner: string;
	shelf_name: string;
	owned: boolean;
};

export type SearchResult = {
	mine: Book[];
	public: PublicHit[];
	shelves: PublicShelf[];
};

export type AdminUserRow = {
	id: number;
	username: string;
	email: string | null;
	is_admin: boolean;
	verified: boolean;
	/** Invited by an admin; password not chosen yet. */
	invited: boolean;
	has_avatar: boolean;
	created_at: string;
	book_count: number;
	shelf_count: number;
};

export type Account = {
	username: string;
	kobo_token: string | null;
};

/* ---- Federation ---- */

export type FedMode = 'off' | 'allowlist' | 'open';

export type FedStatus = {
	available: boolean;
	mode: FedMode;
	host: string | null;
	/** Instances awaiting an admin decision; 0 for non-admins. */
	pending_instances?: number;
};

/** A book that stops a shelf from federating, and why. */
export type BlockingBook = { id: number; title: string; reason: NotFederable };

export type FollowState = 'pending' | 'accepted' | 'rejected' | 'gone';

export type FedFollow = {
	actor: string;
	handle: string;
	name: string;
	summary: string | null;
	state: FollowState;
	book_count: number;
	/** The follow is held until an admin allows the instance. */
	awaiting_approval: boolean;
	/** Set while the shelf's instance does not answer. */
	unreachable_since: string | null;
};

export type FedBook = {
	object_iri: string;
	title: string;
	author: string | null;
	language: string | null;
	isbn: string | null;
	summary: string | null;
	license: string | null;
	license_source_url: string | null;
	author_death_year: number | null;
	epub_size: number | null;
	cover_url: string | null;
	published: string | null;
	imported_book_id: number | null;
};

export type FedAdminSettings = {
	public_url: string | null;
	mode: FedMode;
	contact: string | null;
	max_epub_mb: number;
};

export type FedInstance = {
	domain: string;
	status: 'allowed' | 'blocked';
	software: string | null;
	note: string | null;
	added_at: string;
	last_seen_at: string | null;
};

export type FedInstancePreview = {
	domain: string;
	software: string | null;
	version: string | null;
	contact: string | null;
	federation_mode: string | null;
};

export type FedRequest = {
	domain: string;
	direction: 'in' | 'out';
	detail: string;
	requested_by: string;
	attempts: number;
	first_at: string;
	last_at: string;
};

export type FedOverview = {
	requests: FedRequest[];
	shelves: { id: number; name: string; owner: string; handle: string | null; followers: number }[];
	queue: { length: number; oldest: string | null };
	rejections: { at: string; activity_type: string; actor: string; domain: string; reason: string }[];
};
