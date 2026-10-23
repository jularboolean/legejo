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
	/** Only EPUB is read, tended and converted; the others are kept and handed out. */
	format: 'epub' | 'pdf' | 'cbz';
	file_size: number;
	/** How many things the health check found wrong with the file. */
	health_issues: number;
	has_cover: boolean;
	created_at: string;
	updated_at: string | null;
	/** Reading progress 0–1 (web or Kobo, latest wins); absent when the book is unread. */
	progress_percent?: number;
	/** Minutes the user's Kobo counted reading it; absent when none. */
	kobo_minutes?: number;
	/** Marked as read by hand, whatever the position says. */
	read_at?: string | null;
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

/** The Kobo devices that have synced with an account, and how the last sync went. */
export type KoboDevices = {
	devices: { model: string | null; last_seen_at: string }[];
	last_sync: { at: string; ok: boolean; new: number; changed: number; collections: number; states: number; status: number | null } | null;
	syncs_last_30_days: number;
};

export type ShelfRef = { id: number; name: string };

/** One thing the health check found wrong with an EPUB file. */
export type HealthIssue = {
	code: string;
	/** The server can repair it when asked. */
	fixable: boolean;
	count?: number;
	examples?: string[];
};

export type Health = {
	issues: HealthIssue[];
	/** Codes of what has been repaired in the file. */
	fixed: string[];
};

export type BookDetail = Book & {
	/** What the health check says about the file; null until it has run. */
	health: Health | null;
	/** Removed from the owner's Kobo on the device; left out of the sync. */
	kobo_removed: boolean;
	shelves: ShelfRef[];
	tags: string[];
	/** Whether the book could go on a federated shelf, and if not, why. */
	federable: FederableStatus;
	/** The books just before and after in the series, when you have them. */
	previous_in_series: SeriesNeighbour | null;
	next_in_series: SeriesNeighbour | null;
	/** Highlights and notes the owner has made in the book. */
	annotation_count: number;
	/** How long the book takes the owner to read; null until its text is counted. */
	reading_time: ReadingTime | null;
};

/** A book's length and the reader's pace (readingtime.rs). */
export type ReadingTime = {
	/** Characters, each run of white space one. */
	chars: number;
	/** Characters a minute. */
	pace: number;
	/** Books the reader's Kobo timed that the pace comes from; 0 for the common pace. */
	paced_by: number;
	minutes: number;
};

/** A highlight in a book, with a note when it has one. */
export type Annotation = {
	id: number;
	book_id: number;
	source: 'web' | 'kobo';
	/** Range CFI; null for a passage the reader has not placed. */
	cfi: string | null;
	text: string;
	note: string | null;
	color: string | null;
	/** Where a device put it, as the device said (JSON); null for the reader's own. */
	location: string | null;
	/** Chapter, share of the book and printed page, as the reader found them. */
	place?: Place | null;
	created_at: string;
	updated_at: string;
};

/** Where a passage stands in a book, for reading off a list. */
export type Place = {
	chapter?: string;
	/** 0–1 through the book. */
	percent?: number;
	/** The page of the printed edition, for a book with a page list. */
	page?: string;
};

export type SeriesNeighbour = {
	id: number;
	title: string;
	series_index: number | null;
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
	/** A few of the shelf's books with covers, newest on the shelf first. */
	cover_books: number[];
	/** Whether the viewer follows the shelf, and what is new on it to them. */
	follow: ShelfFollow;
};

export type ShelfFollow = {
	followed: boolean;
	/** Books put on the shelf since the viewer last looked. */
	new_books: number;
	/** When the viewer last looked before this visit. */
	seen_at: string | null;
	/** The viewer's own Atom feed of the shelf. */
	feed: string | null;
};

/** A followed shelf with books on it the viewer has not seen. */
export type ShelfNews = {
	id: number;
	name: string;
	owner: string;
	new_books: number;
	latest: string;
	titles: { id: number; title: string; author: string | null }[];
};

export type PublicBook = Book & {
	owned: boolean;
	/** When the book was put on the shelf. */
	added_to_shelf?: string | null;
};

export type PublicShelfDetail = PublicShelf & { books: PublicBook[] };

export type PublicHit = Book & {
	owner: string;
	shelf_name: string;
	owned: boolean;
};

/** A passage of a book that matches a search in the books' text. */
export type TextPassage = {
	section: number;
	title: string | null;
	/** Where in the book, 0–100. */
	percent: number;
	/** The matched words are between U+E000 and U+E001. */
	text: string;
};

export type TextSearchResult = {
	books: { book: Book; hits: number; passages: TextPassage[] }[];
	/** Books on shelves shared with the user, which the user has no copy of. */
	shared: { book: PublicHit; hits: number; passages: TextPassage[] }[];
	/** Books not yet read into the index. */
	pending: number;
	/** More parts matched than the server looked at. */
	truncated: boolean;
};

export type SearchResult = {
	mine: Book[];
	/** The user's own shelves whose name matches. */
	my_shelves: { id: number; name: string; has_cover: boolean; book_count: number }[];
	public: PublicHit[];
	shelves: PublicShelf[];
	audiobooks: Audiobook[];
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
	/** For password resets and sign-in by email. */
	email: string | null;
	/** A new address waiting to be confirmed from the link sent to it. */
	pending_email: string | null;
	/** Whether the server sends mail: a new address is then confirmed first. */
	mail_configured: boolean;
	kobo_token: string | null;
	/** Where Send to Kindle mails the user's books. */
	kindle_email: string | null;
	/** The address those mails come from; null when the server sends no mail. */
	mail_from: string | null;
	/** Whether the user has turned the librarian on. */
	librarian: boolean;
	/** Whether the server has a librarian to turn on. */
	librarian_available: boolean;
	/** Whether the user has turned the fediverse on. */
	fediverse: boolean;
	/** Whether the instance federates. */
	fediverse_available: boolean;
	/** Whether the user has turned on the catalogs of other libraries. */
	catalogs: boolean;
	/** Whether an admin has turned catalogs on for the instance. */
	catalogs_available: boolean;
};

/* ---- Federation ---- */

export type FedMode = 'off' | 'allowlist' | 'open';

export type FedStatus = {
	available: boolean;
	/** The user has turned the fediverse on for themselves. */
	enabled: boolean;
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
	/** When the book was shared, not when it was published. */
	published: string | null;
	/** The whole description, as Markdown; absent from older instances. */
	description?: string | null;
	publisher?: string | null;
	series?: string | null;
	series_index?: string | null;
	category?: string | null;
	first_published?: number | null;
	/** The date of this edition. */
	edition_date?: string | null;
	/** The book's page on its instance. */
	page_url?: string | null;
	imported_book_id: number | null;
};

export type FedAdminSettings = {
	public_url: string | null;
	mode: FedMode;
	contact: string | null;
	max_epub_mb: number;
	/** Instances worth connecting to, not decided on yet. */
	suggested_instances?: string[];
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

/** One file of an audiobook: a part, in listening order. */
export type AudiobookPart = {
	id: number;
	position: number;
	/** From the file's tags; null when it has none. */
	title: string | null;
	filename: string;
	seconds: number;
	bytes: number;
};

export type Audiobook = {
	id: number;
	title: string;
	author: string | null;
	narrator: string | null;
	language: string | null;
	description: string | null;
	category: string | null;
	tags: string[];
	has_cover: boolean;
	/** The owner's user name. */
	owner: string;
	/** Whether it is the user's own; others may only listen. */
	mine: boolean;
	visibility: 'private' | 'restricted' | 'instance';
	created_at: string;
	updated_at: string | null;
	parts: number;
	seconds: number;
	bytes: number;
};

export type AudiobookDetail = Audiobook & {
	files: AudiobookPart[];
	/** The podcast feed: the address to give a podcast app. Each user has their own. */
	feed_url: string;
	/** The users a restricted audiobook is shared with; empty unless it is the user's own. */
	members: ShelfMember[];
};

/** An OPDS catalog of another library that the user has added. */
export type Catalog = {
	id: number;
	title: string;
	url: string;
	searchable: boolean;
};

export type CatalogFile = {
	href: string;
	format: 'epub' | 'pdf' | 'cbz';
	title: string | null;
	size: number | null;
};

/** A book, when it has files; otherwise a way further into the catalog. */
export type CatalogEntry = {
	title: string;
	authors: string[];
	summary: string | null;
	language: string | null;
	rights: string | null;
	cover: string | null;
	href: string | null;
	files: CatalogFile[];
};

export type CatalogPage = {
	catalog: Catalog;
	url: string;
	title: string;
	entries: CatalogEntry[];
	next: string | null;
	previous: string | null;
};

/** A link to a shelf, as its owner sees it (Private links on the shelf's edit page). */
export type ShelfLink = {
	id: number;
	name: string;
	/** The secret part of the link's address. */
	token: string;
	created_at: string;
	expires_at: string | null;
	last_used_at: string | null;
};

/** A shelf followed by a private link. Its address is kept by the server alone. */
export type LinkedShelf = {
	id: number;
	title: string;
	description: string | null;
	/** The instance the shelf is on. */
	host: string;
	/** Whether it answered when last asked: its instance may be down, or the link taken back. */
	state: 'ok' | 'unreachable' | 'gone';
	state_since: string | null;
	created_at: string;
	/** Whether the shelf has a picture of its own on its instance (/api/linked-shelves/{id}/cover). */
	has_cover?: boolean;
	/** Without one: which drawn cover it has there. */
	cover_seed?: number | null;
};

/** A book on a shelf followed by link, as its feed gives it. */
export type LinkedBook = {
	title: string;
	authors: string[];
	/** Short and plain, for the list. */
	summary: string | null;
	/** The whole description, HTML or text, to be shown with renderMarkdown. */
	description?: string | null;
	publisher?: string | null;
	/** When the edition was published, as the other instance writes it. */
	issued?: string | null;
	categories?: string[];
	rights?: string | null;
	language: string | null;
	cover: string | null;
	files: { href: string; format: 'epub' | 'pdf' | 'cbz'; title: string | null; size: number | null }[];
};
