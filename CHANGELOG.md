# Changelog

What changed in each release of Legejo, newest first. Versions follow
[semantic versioning](https://semver.org); every release is published as a
container image with the same tag.

## 1.19.2 (2026-10-14)

- **Catalogs of other libraries that could not be searched now can.** A
  catalog that gave both a description of its search and the address to
  search with got no search at all when the description could not be
  fetched; the address is now taken first. And an optional part of a search
  address, such as the page to start on, was left empty, which some catalogs
  refuse; it is now filled in. A catalog added before this version keeps the
  search it was given: remove it and add it again to get the search box.
- **Catalogs behind a proxy.** A link written as plain http with port 80 is
  now followed over https on the right port.
- **The librarian** is a little smaller on its page.
- **Upgrading:** nothing to do.

## 1.19.1 (2026-10-13)

- **The app says what the network is doing.** On a poor connection Legejo
  could seem frozen: a press on a link was met with nothing while the page
  was being fetched. Now a line moves along the top while that goes on, a
  note says so when it takes more than a few seconds, and another says when
  there is no connection at all, each with the way to the books saved on the
  device. If the server has not begun to answer in half a minute, the page
  says that it could not be reached instead of waiting without end.
- **A list that could not be fetched is not an empty one.** The library, the
  reading page, authors, a series, audiobooks and shared shelves showed
  nothing when the server answered with an error, as if there were no books.
  They now show the error and a way to try again.
- **Covers no longer blink** with animations on. A cover is only faded in
  when it has kept the page waiting, and the books appear at once when a
  filter changes.
- **The upload's spinner turns again.** With animations off, which is where
  they start since 1.19.0, it stood still.
- **Upgrading:** nothing to do.

## 1.19.0 (2026-10-12)

- **A choice of design.** The Account page has a new part, **Design**, with
  four looks: **Automatic** (light or dark, as your system is set, which is
  what Legejo did before), **Light**, **Dark**, and **Zamenhof**: the browns
  of an old textbook, the green star of Esperanto, the pigeons on a
  medallion, and a book typeface. The choice is kept on the device. The
  reader keeps its own themes.
- **Animations are yours to turn on.** The motion that came with 1.18.1 made
  the app feel slower to some, so it is now off until you tick **Show
  animations** under Design. On, it is clearer than before: menus and dialogs
  come and go, new books fade in, one page fades into the next, and the page
  of a book comes in once it has been laid out. The books no longer slide
  about when the library is filtered or sorted.
- **Covers arrive.** The place of a cover that is still being fetched shows
  a soft shimmer until the picture is there.
- *The Legejo Cookbook* has a new recipe, Choosing How Legejo Looks.
- **Upgrading:** nothing to do.

## 1.18.1 (2026-10-12)

- **A little motion.** Books glide to their places when the library is
  filtered or sorted, menus and dialogs come in rather than appear, the bar
  for selected books rises from the edge, and one page fades into the next
  where the browser can do that. All of it is brief, and none of it happens
  for someone who has asked their system for less motion.
- **A shared shelf's books can be ordered:** by latest added to the shelf,
  author, title or year of publication, either way round, with the same
  control as in My library. The choice is remembered on the device.
- **Room in the library's heading on a phone.** The buttons for selecting
  and for uploading books show their icons only there.
- **Upgrading:** nothing to do.

## 1.18.0 (2026-10-12)

- **Where you are in the browser, the Kobo opens.** A book read in the web
  reader opens on a Kobo at the same sentence after the device's next sync,
  as it already did the other way round. The web reader sends a few words
  from its position along with the place; the server finds them in the copy
  the device reads. The Kobo's own later position, and its reading time,
  stay its own. A book the device has not downloaded yet gets the place with
  the download.
- **Read, or started over, by hand.** A book's page has **Mark as read**,
  whatever the position says, and for a book begun, **Start over**, which
  forgets the place in it here, in KOReader and on a Kobo. **Mark as unread**
  takes back a mark. The reading page and the statistics count a book marked
  read as finished. A book marked unread on the Kobo counts as not begun here
  too.
- **Back to the page you asked for.** A link that needs a login leads to its
  page once you are logged in, with single sign-on as well.
- **What is new.** After an upgrade, the library says so once, with a link to
  what changed; the Admin page shows the same list on demand.
- **Your Kobo devices.** The Account page lists the devices that have synced,
  when each was last seen, and how the last sync went: how many books,
  collections and reading states it got, or what failed.
- **KOReader on a Kobo.** The file Kobo sync hands the device is noted under
  the book, so KOReader on that device finds the book by the file and syncs
  its progress with Legejo too. A comic made into a book for the device
  likewise. The conversion for the device has a time limit and runs two at a
  time at most.
- Highlights made in the web reader are found in the device's copy of the
  book whatever its line breaks; a few that were not placed before are now.
- *The Legejo Cookbook* has a new recipe, for marking a book as read or
  starting over, and says how a place read in the browser reaches a Kobo.
- **Upgrading:** nothing to do. Two columns are added to the tables for
  books and reading positions.

## 1.17.1 (2026-10-12)

- **Improved eyes on pigeons.** Both birds now see the library through
  yellow eyes, and the one on the app's icon has a lighter wing, to match
  its companion. The change is carried through to the app's icons, the pages of federated shelves and
  the cover of *The Legejo Cookbook*. No pigeons were harmed.
- **The app no longer starts as a blank screen.** Opened from the home
  screen, Legejo could show nothing but its background for a couple of
  seconds while it loaded. It now shows the pigeons while they wait with
  you, in light and in dark. Where the app starts at once, nothing is shown.
- **Upgrading:** nothing to do. To get the new icon on a phone, remove
  Legejo from the home screen and add it again.

## 1.17.0 (2026-10-11)

- **A shelf for someone on another Legejo.** The owner of a shelf can make
  private links to it, on the shelf's edit page under **Private links**: an
  address that opens the shelf without a login, one for each person it is
  given to. Each can be given a last day, shows when it was last used, and
  can be taken back on its own, at which it stops working at once.
  - Pasted into another Legejo, at the foot of **Shared shelves**, the link
    becomes a shelf there, under **Shelves from other Legejo instances**,
    with its books to be added to that library. The list marks a shelf whose
    instance is not answering, and one whose link no longer works.
  - The link is an ordinary OPDS catalog, so it also works in KOReader,
    Thorium and other reading apps.
  - Off until an admin turns on **Shelves by private link**. With it on, the
    server hands out the books on a shelf to whoever holds a link to it, and
    fetches shelves from the addresses its users follow. It is meant for
    people you know.
- The login page shows the pair of pigeons, the name and the version.
- *The Legejo Cookbook* has two new recipes, for giving a shelf away by link
  and for following one. A new instance gets the new edition; the file is in
  the repository as `server/assets/the-legejo-cookbook.epub`.
- **Upgrading:** nothing to do. The database gains two tables on first
  start.

## 1.16.0 (2026-10-11)

- **Reading without the network.** With Legejo on your home screen (or in
  the Dock), a book can be saved on the device: **Save on this device** on
  the book's page. The app then opens without the network and shows the
  books that are there, under **On this device**. Your place in them is
  kept and sent when the server can be reached again; if you read further
  on another device meanwhile, the later place wins. Good to know:
  - On iPhone, iPad and in Safari on a Mac the button is offered only in
    the installed app. In a tab, Safari empties what a site has stored
    after a week unvisited; the page says so instead.
  - Without the network, highlights can be read but not made, and the rest
    of the library cannot be browsed.
  - Logging out removes what was saved on the device.
- **A new instance begins with its manual.** *The Legejo Cookbook*, the user
  manual as a book of recipes, is put in the first admin's library when an
  instance is first started. It is in the public domain, and can be deleted
  like any book. Existing instances do not get it; it is in the repository
  as `server/assets/the-legejo-cookbook.epub` for anyone who wants it.
- **Upgrading:** nothing to do. The web app now installs a service worker,
  which is what keeps the app's own files on the device.

## 1.15.0 (2026-10-10)

- **Highlights and notes in the reader.** Select a passage and keep it,
  with a note if you like. A panel (key `N`) lists them, jumps to each and
  exports them as Markdown; the book page says how many a book has. Where
  text cannot be selected (Safari, and the app on iPhone, iPad and Mac),
  drag with the mouse or hold a finger on a sentence. A tap on a highlight
  opens it, and with a mouse its note shows beside the pointer.
- **Highlights sync with a Kobo, both ways.** An admin turns it on under
  Settings ("Highlights from Kobo e-readers"); it is off by default. What
  you highlight and note on the device then lands in Legejo, placed in the
  book; what you highlight in the web reader, and notes you change there,
  reach the device at its next sync. Removing one removes it on the other
  side too. Good to know:
  - It has been tried on one device, a Kobo Clara 2E with firmware 4.38.
  - With the setting on, the device sends its highlights to Legejo instead
    of to Kobo, for all its books. Those of books bought from Kobo are not
    taken in and no longer reach Kobo, and a device that holds such books
    has not been tried: leave the setting off if yours does.
  - Highlights already on the device before the setting was turned on are
    not sent, unless they are changed on the device.
  - The device must sync once after the setting is turned on before its
    highlights are taken in.
- The panel with keyboard shortcuts in the reader opens in Safari, where it
  had no height.
- `LEGEJO_KOBO_ANNOTATIONS_LOG`, the groundwork in 1.14.3 to 1.14.5, is
  gone; the setting above replaces it.
- **Upgrading:** nothing to do. The database gains three tables and a
  column on first start.

## 1.14.5 (2026-10-10)

- The Kobo groundwork from 1.14.3 names its key the way the device spells
  it (`reading_services_host`); the earlier spelling was ignored. Each call
  is answered with the empty shape the device expects, so a sync is never
  aborted by it. Still off unless `LEGEJO_KOBO_ANNOTATIONS_LOG` is set.

## 1.14.4 (2026-10-10)

- The Kobo groundwork from 1.14.3 names the server by its host alone, which
  is the form the device accepts. Still off unless
  `LEGEJO_KOBO_ANNOTATIONS_LOG` is set, and still nothing is stored.

## 1.14.3 (2026-10-10)

- **Groundwork for syncing highlights from a Kobo.** With
  `LEGEJO_KOBO_ANNOTATIONS_LOG=true`, a Kobo is told to send its highlights
  and notes to Legejo, which logs what arrives and stores none of it. This
  is for finding out what the device sends; it does nothing unless the
  variable is set, and nothing is kept either way.
- The web app's dependency `@xmldom/xmldom` is held at a version without
  known vulnerabilities. It is only used by the reader in a browser without
  `DOMParser`, which none is.
- **Upgrading:** nothing to do.

## 1.14.2 (2026-10-10)

- **Is there a newer Legejo?** The admin page has a button that asks
  GitHub for the latest release and says whether this instance is behind,
  with a link to what is new. It asks only when the button is pressed, from
  the admin's browser; the server never asks anyone anything.
- **One tap opens a book on a touch screen.** The download icon that
  appeared on a gallery card when hovered made the first tap on a phone a
  hover instead of a tap. The gallery no longer has it (the book page has
  the download), and the list's icon hides until hovered only where
  hovering exists.
- **SECURITY.md:** how to report a vulnerability, privately, and what counts
  as one.

## 1.14.1 (2026-10-10)

- **A note when a new version is ready.** When a newer Legejo has been
  published, a small note at the bottom of the page says so, with a button
  that reloads. It appears within a minute while the page is open, and at
  once when you come back to it. Until then, the next page change loads
  the new version by itself. This is how an installed home-screen app gets
  a new version too: there is nothing to pull down.
- **Fixed:** deleting a book, an audiobook or a shelf flashed "not found"
  for a moment before the library appeared.

## 1.14.0 (2026-10-10)

- **Shared shelves as a gallery or a list.** The list shows each shelf's
  description, how many books it holds, and a glimpse of its covers, fanned
  out over each other. The choice is remembered on the device, as in the
  library.
- **Next in the series.** The book page points at the books just before and
  after in the series, when you have them.
- **A shared book takes space once.** Fetching a book from a shared shelf
  into your library no longer copies the file: on the same disk, the two
  libraries share it until one owner changes their copy. Books fetched
  before this release stay as they are.
- **On a phone:** a swipe to the right across the page opens the menu, and
  one to the left closes it. Text fields no longer make the page zoom in
  when you tap them.
- **Upgrading:** nothing to do.

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
