# Changelog

What changed in each release of Legejo, newest first. Versions follow
[semantic versioning](https://semver.org); every release is published as a
container image with the same tag.

## 1.13.0 (2026-10-09)

- **Legejo as an app on your phone.** Add Legejo to the home screen of an
  iPhone, iPad or Android device (in Safari: Share, then Add to Home Screen),
  and it opens as an app of its own, without the browser's address bar, with
  the pigeon as its icon. Desktop browsers offer to install it too. The
  status bar takes the library's colour, light or dark, and the reader's own
  colour while you read. This is the first step; opening the app without a
  network connection is not part of it yet.
- **Upgrading:** nothing to do. The old home-screen bookmark, if you had one,
  keeps the old icon; add it again to get the new one.

## 1.12.0 (2026-10-09)

- **Catalogs of other libraries (optional):** add the OPDS catalog of
  another library, such as Project Gutenberg, by its address. Look through
  it and search it from Legejo, and fetch a book into your library with one
  click.
  - Off until an admin turns it on (Admin, Settings), and then for each user
    until they turn it on from their account page, under Options. With it
    on, the server fetches from addresses its users give it; only public
    `https` addresses are fetched from, never ones inside the server's own
    network.
  - Six catalogs of free books are suggested to begin with: Project
    Gutenberg and Unglue.it (English), Ebooks libres et gratuits and
    Bibliothèque numérique romande (French), Izzy’s freie Bibliothek
    (German and English) and textos.info (Spanish). Any other OPDS catalog
    is added by its address.
  - A fetched book is an ordinary book: checked and repaired like an
    uploaded one, and not added again when you already have the file. No
    licence is set on it; what the catalog says about the rights is shown
    with each book.
  - A book with several files, such as an EPUB with and without images, has
    a button for each.
  - Covers reach your browser by way of the server, so the browser never
    talks to the catalog itself.
  - Legejo reads OPDS 1 catalogs that are open to everyone. Catalogs that
    ask for a login, and OPDS 2, are not read. When a catalog refuses a
    page, has none at the address or is slow to answer, Legejo says which.
- **Upgrading:** nothing to do.

## 1.11.0 (2026-10-09)

- **The fediverse is something each user turns on.** On an instance that
  federates, the Fediverse page stays out of the menu until a user turns it
  on from their account page, under Options. Federating one of your shelves
  turns it on by itself. Users who already follow a shelf or federate one
  keep it on after the upgrade.
- **Account and admin pages in tabs.** The account page is split into
  Profile, Devices and apps, and Options; the admin page into Settings,
  Users, Federation and System log. Each tab has an address of its own.

## 1.10.0 (2026-10-08)

- **Import folder for audiobooks:** set `LEGEJO_AUDIOBOOK_IMPORT_DIR` to a
  folder on the server, and what is dropped into it becomes audiobooks of
  `LEGEJO_AUDIOBOOK_IMPORT_USER` (default: the oldest admin). Each entry at
  the top of the folder is one audiobook: a folder with its mp3, m4a or m4b
  files, or a single audio file.
  - Subfolders such as `CD 1` are included, and the parts come in the order
    of their names, numbers by their value.
  - Title and author come from the files' tags, otherwise from the name of
    the folder. An image beside the files becomes the cover.
  - The audio is never held in memory, and nothing is copied when the
    folder is on the same disk as `/data`, so audiobooks of several
    gigabytes are fine.
  - Afterwards an entry is moved to `imported/`, `duplicates/` or
    `failed/` in the folder, or deleted with
    `LEGEJO_AUDIOBOOK_IMPORT_DELETE`. Audiobooks must be turned on at the
    admin page.
- **The librarian** has a larger portrait, to the right of the question.

## 1.9.0 (2026-10-07)

- **The librarian (optional):** ask for something to read in your own
  words, such as "something short and funny I have not read", and get a
  handful of books from your library and from the shelves others share
  with you. The librarian is a language model that the operator of the
  instance sets up and pays for (`LEGEJO_AI_API_KEY`, with
  `LEGEJO_AI_BASE_URL` and `LEGEJO_AI_MODEL` to choose the provider; any
  service with an OpenAI-style chat API works).
  - It is off for every user until they turn it on from their account
    page.
  - What is sent to the model is the question and what the catalogue says
    about the books: title, author, shelves, tags, reading status and the
    beginning of the description. The books themselves are never sent.
  - The model can only point at books in the catalogue it was given;
    nothing it writes is shown.
  - Each user can ask 30 questions an hour, and the admin page shows how
    many were asked and how many tokens they took. The cost of a question
    grows with the size of the library.
- **Search:** when a search finds nothing and the librarian is on, the page
  offers to ask the librarian instead.

## 1.8.0 (2026-10-07)

- **Comics on a Kobo:** Kobo sync now brings your comics (CBZ) to the
  e-reader. Legejo makes each one into a book the device reads, one page
  per image in reading order, with the images as they are; manga marked as
  such in `ComicInfo.xml` is read from the right. The comic in your library
  is not changed. PDF files still stay out of Kobo sync.
- **Upgrading:** the comics already in a library get today's date as the
  day they were added, so that they reach an e-reader that has synced
  since; they show first under "recently added". The book made for the
  device is kept on disk beside the comic, so a synced comic takes about
  twice its size.

## 1.7.0 (2026-10-07)

- **PDF and comics (CBZ):** these files can now be kept in the library next
  to your EPUB books: upload them, or drop them in the import folder, and
  put them on shelves, tag, search, share, download and export them like
  any other book. Reading apps get them over OPDS.
  - A comic takes its first page as the cover and reads title, series,
    number, writer and language from a `ComicInfo.xml` when it has one.
  - A PDF gives its title and author, and a scanned one its first page as
    the cover. Password-protected PDFs are refused.
  - Legejo keeps these files as they are. The web reader, reading aloud,
    the file check, writing corrections back into the file and Kobo sync
    remain EPUB only, and so does sharing over the fediverse.
  - The library marks them with their format and can filter on it. Send to
    Kindle works for PDF.
- **Upgrading:** nothing to do. Going back to an earlier version afterwards
  works, but that version cannot open the PDF and CBZ books added meanwhile.

## 1.6.1 (2026-10-07)

- **Fixed:** the system log (Admin) did not open once an audiobook had been
  added or deleted. Those entries now have texts, the log has a filter for
  audiobooks, and an entry the page has no text for is shown by its code
  rather than stopping the page.

## 1.6.0 (2026-10-07)

- **Send to Kindle:** give the address of your Kindle on your account page,
  and every book of yours gets a button that mails it there. Needs mail to
  be set up on the server (`LEGEJO_SMTP_*`); the account page names the
  sender address to approve with Amazon. Files over 25 MB are not sent.
- **Search:** finds your own shelves by name and books by their tags, names
  the language of every book and audiobook found, and the shelf a shared
  book is on is now a link to that shelf.
- **Audiobooks:** the cover and the details sit side by side on the
  audiobook's page.
- **Fixed:** the cover of an audiobook did not show in some podcast apps.
  The feed now gives it at an address that ends in the image type, in both
  of the forms apps look for. An app that already follows the feed may need
  the feed added again.

## 1.5.0 (2026-10-07)

- **Audiobooks (optional):** an admin can turn on audiobook support in the
  admin settings. Upload MP3, M4A or M4B files, or a whole folder; each audiobook
  becomes a private podcast feed with one episode per file. Add the address
  in a podcast app, or use the buttons for Apple Podcasts, Pocket Casts,
  Overcast and Castro, and the app keeps your place. Parts can also be
  played on the audiobook's page.
- **Sharing audiobooks:** keep an audiobook to yourself, share it with the
  users you choose, or with everyone on the instance. Every listener has a
  feed address of their own, which stops working when the sharing ends.
- **Finding audiobooks:** category and tags, a filter bar with text search
  (title, author, reader, description), a gallery and a list view, and the
  search over everything finds audiobooks too.
- **Sidebar:** Search is now the first entry.

## 1.4.1 (2026-10-06)

- **AI assistants (MCP):** two new read-only tools list the shelves other
  users share with you and the books on them, so an assistant can suggest
  what is worth importing. The text of those books stays out of reach until
  you have imported them.

## 1.4.0 (2026-10-06)

- **Read aloud:** the web reader reads the book with the voices your device
  already has. The sentence being read is marked, the pages turn along, and
  reading continues into the next chapter. A bar has play and pause, a
  sentence back and forward, speed and the choice of voice. Shortcut: `L`.
- **Page-turn animation:** an optional short glide when the page turns, off
  by default (reader settings).
- **Fixed:** books that hide text for screen readers far outside the page
  (all of Standard Ebooks, among others) no longer get dozens of empty
  pages in chapters such as the title page and the imprint.

## 1.3.2 (2026-10-06)

- **Fixed:** during an update, a browser could be told that a new script
  file did not exist, and caches in front of the server kept that answer
  for a year, so the page stayed blank. A missing file is no longer cached.

## 1.3.1 (2026-10-06)

- A cover you have chosen in Legejo is now put into EPUB files that had no
  cover of their own, when you save or repair the book.
- **Repair files** for a whole selection of books at once.
- The health check shows only what you can do something about. A dead link
  or a missing picture is listed under "Details about the file".
- **Fixed:** the Open Library lookup said "Libris" in its messages.

## 1.3.0 (2026-10-06)

- **Health check on upload:** a missing table of contents, language,
  identifier or cover declaration is repaired before the file is stored,
  as are references to files that are not there. What cannot be repaired is
  reported on the book's page, and the library can be filtered on it.
  Books you already have are checked in the background and repaired when
  you ask.
- **The file follows the catalog:** saving a book writes title, authors,
  language and series into the EPUB itself.
- **Copy-protected (DRM) files are refused** at upload, with an explanation.
- A book whose file claims copyright is marked as protected from the start.
- **Upload progress:** files are sent one at a time, each with its own
  progress and result. The size limit now applies per file.

This release adds a database migration. It only adds columns and a table.

## 1.2.2 (2026-10-06)

- **Fixed (federation):** accounts on servers that require signed requests,
  such as mastodon.social, could not follow a shelf.
- Announcements of deleted accounts from servers that have never been in
  touch are no longer listed as rejected activities.
- The list of rejected activities can be cleared.

## 1.2.1 (2026-10-05)

- **Federation:** a book posted to followers now carries its author and
  description instead of its license, and its cover as an attachment, so
  it shows properly in Mastodon.
- The public page of a federated shelf lists each book's description and
  has the look of the application.
- A server can now start on a database that a newer version has migrated,
  which makes rolling back a release possible.

## 1.2.0 (2026-10-05)

- **Restricted shelves:** share a shelf with the users you choose, beside
  private, everyone on the instance and federated.
- "Public shelves" are now called "Shared shelves".

This release adds a database migration. It only adds a column and a table.

## 1.1.3 (2026-10-05)

- The image has a health check built in, and the README describes running
  Legejo on Coolify, Dokploy and similar platforms.

## 1.1.2 (2026-10-05)

- **AI assistants (MCP):** the app password can be sent in an `X-Api-Key`
  header, for clients that reserve `Authorization` for OAuth.

## 1.1.1 (2026-10-05)

- **Fixed (MCP):** clients speaking the 2026-07-28 revision of the protocol
  rejected the tool list.

## 1.1.0 (2026-10-05)

- **AI assistants (MCP):** a read-only Model Context Protocol server lets an
  assistant you already use search your library, look up books, shelves and
  reading progress, and read or search the text of a book. Off by default;
  turn it on with `LEGEJO_MCP=true`.

## 1.0.0 (2026-10-05)

First release.
