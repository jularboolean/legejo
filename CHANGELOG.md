# Changelog

What changed in each release of Legejo, newest first. Versions follow
[semantic versioning](https://semver.org); every release is published as a
container image with the same tag.

## 1.26.0 (2026-10-21)

Legejo is Esperanto for a place where one reads, and this release makes
something of that.

- **Search understands the x-system.** Esperanto typed without its own
  letters finds the words as printed: `cxambro` finds *ĉambro*, in the
  library and in the text of the books.
- **The manual in Esperanto.** *La Kuirlibro de Legejo*, with a cover of
  its own. In English or Esperanto, the account page under Language puts
  the manual in that language in your library, so instances started
  earlier get it too.
- **Look a word up while reading.** Select a word in the web reader and
  choose *Look up*: Esperanto from Reta Vortaro, the open Esperanto
  dictionary, with the word in your own language and in English; other
  languages from Wiktionary. A word is looked up as the dictionary has it
  (*ĉambrojn* as *ĉambro*). The server asks on the reader's behalf. An
  admin setting under Library, off by default.
- **Shelves worth following.** The Fediverse page suggests two federated
  shelves, five Esperanto classics and a shelf of gothic fiction, and the
  admin page offers their instance beside *Connect an instance*.
  `LEGEJO_SHELF_SUGGESTIONS=off` hides them.
- **The public page of a federated shelf** follows the design chosen in
  Legejo on that device, dark or Zamenhof, not only the system's setting;
  shows its description and the books' as formatted Markdown; names the
  owner as "by …"; and keeps the note about Legejo to a small footer.
- **Change your email address.** The account page has an email field.
  Where the server sends mail, a new address is used once the link sent to
  it is opened, and the old address is told of the change.
- **The Esperanto interface is complete**, with the texts it lacked for the
  search inside the books and the Kobo's reading time.

## 1.25.0 (2026-10-21)

- **The Kobo store beside Legejo.** A new admin setting, under Kobo on the
  admin page, lets a Kobo that syncs with Legejo keep its Kobo account: the
  store works on the device again, and books bought from Kobo, or borrowed
  through Kobo Plus, arrive beside your own, with their reading places and
  highlights kept by Kobo. Legejo answers for its own books and passes what
  concerns Kobo's on to Kobo, with the device's own Kobo sign-in. If Kobo
  cannot be reached, Legejo's books still sync and Kobo's come later; the
  highlights of a bought book are never answered as "none" in the meantime,
  so the device keeps them. Off by default.
- **The admin settings are grouped.** Metadata, Kobo, Library, Sharing and
  Accounts each have a heading of their own instead of one long list.
- **Libris lookup is off on a new instance**, like the Open Library lookup,
  until an admin turns it on. An instance where the settings were ever saved
  keeps its choice. Its description now says what Libris is: Sweden's
  national library catalogue.
- **A shelf's description fits a phone.** A long word or address in it no
  longer makes the page wider than the screen, and the sort and view buttons
  go below the description when there is no room beside it.

## 1.24.1 (2026-10-19)

- **Searching the text sits beside the search box.** On a wide screen the box
  for searching inside the books stands next to the search field instead of
  below it; on a phone it wraps below.
- **The admin page fits a phone.** The buttons under Version go onto a line
  each when there is no room, instead of making the page wider than the
  screen.
- **What a Kobo gives up, said plainly.** The handbook and the README now
  say that the Kobo store does not work on a Kobo while it syncs with Legejo,
  and that books bought from Kobo do not arrive; the handbook says how to
  switch back.

## 1.24.0 (2026-10-18)

- **The Kobo's reading time.** The time a Kobo counts reading a book shows on
  the book's page, and the time over all books on My reading. AI assistants
  see it in the book details too. Marking a book read or unread in the web no
  longer resets the count on the device.
- **The browser opens at the Kobo's line.** Read on a Kobo, then open the book
  in the browser: it opens at the sentence you stopped at on the device, not
  only at the same percentage. This works for books the Kobo got from
  Legejo; for others the percentage stands, as before.

## 1.23.1 (2026-10-17)

- **The admin page works with PostgreSQL and the text search on.** Its
  account of the search inside the books came back from PostgreSQL in a form
  the server could not read, so the whole settings page failed to load and
  showed defaults instead. The account is read correctly now, and should it
  ever fail again, the rest of the settings load all the same.
- **The features on the admin page are evenly spaced.** Some switches sat
  close under the explanation before them and others further down; now each
  feature is the same distance from the one before.

## 1.23.0 (2026-10-17)

- **Searching inside the books.** Set `LEGEJO_FULLTEXT=true` and the search
  page gets a box for searching the text of the EPUBs, not only their titles
  and authors. Each book comes with the passages that match and where they
  are; choose one to open the book with the search already in it. Every word
  must occur, a word also finds longer words it begins, and words in double
  quotes must stand in a row. Books on shelves shared with you are searched
  too, and quoted, until you take them.
- **The text is kept once.** The server reads the books in the background
  and keeps their text in the database: once per text, so an imported copy
  takes no room of its own, and a book whose details are edited is not
  stored again. The admin page shows how far it has come and how much text
  is kept. Started without the variable, the server drops the text again.
- **An assistant can search all the books.** With the search turned on, the
  MCP server offers `search_text`, which finds words in the text of the whole
  library at once. It is still read-only.
- **The reader opens with a search.** A link to a book can carry the words to
  look for; the reader opens with the search panel showing them.

## 1.22.2 (2026-10-16)

- **Book files are sent as they are read.** A download to a Kobo, to the
  browser, through OPDS or through a private link no longer loads the whole
  book into the server's memory first. A comic made into a book for the
  device can be hundreds of megabytes, and two devices fetching at once held
  two such files; now each takes a small buffer, whatever its size.
- **A file that cannot be read no longer drops the connection.** An EPUB with
  a stray unclosed tag in its package document made the server's file check
  fail in a way that cut the upload off without a word. The check now reads
  past such a tag, and the reading of an uploaded book runs off the request
  threads, so a bad file ends in a message to the uploader, and a large one
  no longer holds up other requests while it is read.
- **Mailed links use the instance's address.** The links in verification and
  password-reset mails are built from `LEGEJO_PUBLIC_URL` when it is set, as
  invitation links already were, rather than from the address the request
  came in on.
- **One round of requests per page load.** The app installed on a device
  asked the server for the session, settings and shelves twice each time a
  page loaded, once for the page and once to warm its offline cache; the
  cache already had them. Now once.
- **Pictures are checked before they are kept.** A cover or an avatar must
  decode as a picture (JPEG, PNG, WebP or GIF), is stored under the type
  found in the file rather than the one the browser claimed, and is served
  with headers that keep a browser from treating it as anything but a
  picture. An avatar is scaled to 512 pixels and stripped of its metadata. A
  cover an EPUB declares as an SVG is served as one, under a policy that
  lets it draw and nothing more.
- **Only the routes that take files accept large bodies.** Books, audio
  files and pictures have their own limits; every other request is capped
  at a megabyte. Past the limit an upload is told it is too large.
- **Passwords are checked off the request threads**, so a burst of logins
  no longer slows down Kobo sync and downloads, and a login for a name that
  does not exist takes as long as a wrong password.
- **Mails from the login page are rationed:** three verification or reset
  mails per address within the login window, and twenty per client address
  when the instance knows client addresses. Asking for a reset twice within
  the hour sends the same link again instead of voiding the first, and the
  mail goes out after the answer. A name is text in a mail, never markup.
- **Expired sessions and unconfirmed registrations are removed daily**, so
  a name registered but never confirmed is free again after two days.
- **One rule for usernames** wherever one is chosen or changed: 1 to 60
  characters, none of them control characters.
- **Upgrading:** nothing to do.

## 1.22.1 (2026-10-16)

- **Covers that went missing.** Some EPUB 3 files name their cover only in
  the older way, and such a book came into the library without one. Legejo
  now finds the cover on upload, and marks it in the file so that a Kobo and
  other reading apps find it too. Books already in the library get their
  cover back after the upgrade, by themselves; their page offers **Repair
  the file** to mark it in the file as well. A cover you chose yourself is
  left as it is.
- **Lists look like the fields beside them.** In Safari, the lists for
  language, licence and federation were drawn as Safari's own small grey
  controls. Every list in the app is now drawn alike, in every browser and
  design.
- **Upgrading:** nothing to do. Every book's file is checked once more after
  the upgrade, which takes a moment on a large library.

## 1.22.0 (2026-10-16)

- **Following a shelf.** A shelf someone shares with you has a **Follow**
  button. Books that come onto a shelf you follow are then announced above
  your library, with the shelf, how many and their titles, and the number
  shows beside **Shared shelves** in the sidebar. On the shelf itself, the new
  ones are marked **New**. Opening the shelf counts as having seen them, and
  **Mark as seen** clears them all at once.
  - Each shelf you follow has an **Atom feed** of its own, for a feed reader:
    an address only you have, which needs no login and stops working when you
    stop following, or when the shelf is no longer shared with you.
- **Your reading, to take elsewhere.** The Account page gives **Reading as
  CSV (Goodreads)**: the books you have read, are reading or want to read,
  with ratings, the date you finished and your shelves, in the form that
  StoryGraph, BookWyrm and Goodreads take in when importing.
- **A new password without mail.** An admin can make a **password reset
  link** for a user on the Admin page, under Users, and pass it on; it works
  once, for a day. When no admin can log in, `legejo reset-password <user>`
  run on the server (with Docker Compose: `docker compose exec legejo legejo
  reset-password <user>`) prints such a link. Both are noted in the system
  log.
- The export on the Account page is now headed **Export your library**.
- *The Legejo Cookbook* has three new recipes: Following a Shared Shelf,
  Taking Your Reading Elsewhere, and Helping Someone Who Has Lost Their
  Password.
- **Upgrading:** nothing to do. A table is added for the shelves people
  follow.

## 1.21.1 (2026-10-15)

- **You see what you pick in Safari.** In Safari, and in the app on a home
  screen, text could only be picked blind: the reader has to lay a cover over
  the book there, and the browser draws no selection under it. Now the
  passage is marked as you drag the mouse, the pointer is a text cursor over
  the page, and on a phone a finger held on a word and then moved picks from
  there to where it is, marked as it moves.
- **From a book to its notes.** The count of highlights on a book's page
  leads to that book on My notes.
- **Upgrading:** nothing to do.

## 1.21.0 (2026-10-15)

- **My notes.** A new page in the sidebar gathers every highlight and note
  from all your books: book by book, in the order they come in the text, the
  book you last changed something in first. Each shows the passage, your
  note set apart below it, and in small grey the chapter, how far through
  the book it is, and the page of the printed edition when the book has a
  page list. Search them all, show only those with notes, write or change a
  note right there, or remove a highlight. **Show in the book** opens the
  reader at the passage, marked for a moment, without moving your place in
  the book, and leads back to your notes. **Export all** gives every book's
  notes in one Markdown file. Highlights from a Kobo are there too.
- Highlights made before this version get their chapter and place the next
  time their book is opened in the reader.
- **A click on a highlight opens it.** On a computer, a click just beside a
  highlight's lines, or in the gap between them, could turn the page
  instead.
- *The Legejo Cookbook* tells of My notes, in Going Through Your Notes.
- **Upgrading:** nothing to do. A column is added for where a highlight
  stands.

## 1.20.2 (2026-10-15)

- **The app holds when the server is down, not only the network.** With the
  network there but the server behind it down, the app installed on a phone
  showed an error, or asked you to log in, and "Books on this device" did
  nothing. Now it opens on the books saved on the device, as it does without
  a network, and finds its way back once the server answers again.
- On a poor connection just after an upgrade, the app no longer mixes the
  page of the new version with the files of the old one, which made every
  page change fail. What could not be fetched is called that, not an
  internal error, and the links on the error page always do something.
- **Upgrading:** nothing to do. The app on a phone takes the change the next
  time it is opened with the server in reach.

## 1.20.1 (2026-10-15)

- **The page of a book stays inside a phone's screen.** On a narrow screen,
  a chapter with a long title made the reader's page a little wider than the
  screen, and the right edge of every line was cut off. The page now keeps to
  the width of the screen, whatever the line at the bottom says.
- **Upgrading:** nothing to do.

## 1.20.0 (2026-10-15)

- **Who an author was.** Filtered on one author, the library says beside
  their name when they lived, who they were in a few words, and links to
  Wikipedia in your language, from Wikidata. It appears only for a person
  Wikidata knows for certain: an author with a common name, or one it does
  not know, gets no line rather than the wrong one. If it is about someone
  else after all, **Wrong person?** hides it for you, with a moment to undo
  it, and **Show again** brings it back. Off until an admin turns on
  **Authors from Wikidata**: the server then asks Wikidata about the authors
  whose books are opened, by name, and keeps the answers for months. Your
  browser never asks Wikidata itself, and a page never waits for it.
- **The selection is only what you see.** Books selected and then filtered
  out of sight left the selection only in name: a change to the selection,
  such as removing it, reached them too. Now they leave it.
- Two files of the same name among an upload's duplicates no longer stop
  the list from showing, an upload cut off no longer leaves the upload
  button locked, a change to many books that worked is no longer reported
  as a network error, and the authors page counts a book marked read by
  hand as read.
- *The Legejo Cookbook* tells of the line about an author, in Browsing by
  Author.
- **Upgrading:** nothing to do. Two tables are added for what Wikidata says.

## 1.19.3 (2026-10-14)

- **Open Library beside the ISBN.** A book in English, or in no stated
  language, links to Open Library from its ISBN. Books in other languages get
  no link: Open Library finds few of them by ISBN.
- **Rating doves look like doves again.** Since the pigeons were redrawn in
  1.17.1, a filled dove in a rating came out black, with its beak, eye and
  wing in a heap in a corner. It is drawn as the logo is again, and the empty
  doves are easier to see on a dark page.
- **Your own shelves can be ordered** like shared ones: by latest added to the
  shelf, author, title or year, with the choice remembered for both.
- **Covers turn a little** under the pointer when animations are on, like a
  book taken off a shelf: on a book's page, and less in the library and on
  shared shelves.
- **The edit button on a phone** stands beside the way back to the catalogue,
  so that the title has the line to itself.
- **Upgrading:** nothing to do.

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
