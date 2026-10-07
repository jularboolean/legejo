# Changelog

What changed in each release of Legejo, newest first. Versions follow
[semantic versioning](https://semver.org); every release is published as a
container image with the same tag.

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
