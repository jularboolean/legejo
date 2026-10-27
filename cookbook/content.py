# The text of The Legejo Cookbook. Each chapter is a title, an opening and a
# list of recipes; a recipe is (title, problem, solution, discussion), the
# last three as XHTML. build.py numbers them and makes the EPUB.


PREFACE = """
<p>Legejo is a library for your e-books that you run yourself. You upload your books, put
them on shelves, and read them in the browser, on a Kobo, in KOReader or in any reading app that
speaks OPDS, and your place in each book follows you from one to the other.</p>
<p>This book came with your library. It was put on the shelf when Legejo was installed, so
that the first book you open can be the one that explains the rest. Delete it when you no longer
need it; nothing depends on it.</p>

<h2>Who this book is for</h2>
<p>Two kinds of reader. Most chapters are for anyone with an account: how to fill the library,
find things in it, read, and get books onto a device. Chapter 1 and Chapter 12 are for the
person who runs the instance, and assume a terminal and some patience with Docker.</p>

<h2>How it is organized</h2>
<p>As a cookbook. Each recipe states a <em>Problem</em>, gives a <em>Solution</em>, and where
there is more to say, a <em>Discussion</em>. Read it front to back once, or look up the one thing
you came for.</p>

<h2>Conventions used in this book</h2>
<p><strong>Bold</strong> is something you click or tap, by the name it has on the screen.
<code>Constant width</code> is something you type, the name of a setting, or an address. Names of
buttons are given in English; Legejo itself also speaks Swedish, Finnish, German, French, Spanish
and Esperanto, and Recipe 11.1 says how to change it.</p>
<p class="note"><strong>Optional features.</strong> Several parts of Legejo are off until
someone turns them on: the Open Library and Libris lookups, audiobooks, catalogs of other libraries, the librarian, federation,
shelves by private link, highlights from a Kobo, the Kobo store beside Legejo. A recipe says so when it concerns one of them. If a button described here
is missing on your screen, that is the likely reason, and the person who runs your instance is the
one to ask.</p>

<h2>What Legejo is not</h2>
<p>It is worth knowing early. Legejo is built around EPUB: that is the format it opens in the
browser, checks, repairs, converts for Kobo and reads aloud. PDF files and comics open in the browser too,
plainly. Legejo also keeps its own copy of every
book. It does not read an existing folder of books in place, and it does not follow changes made
there. The Appendix says more.</p>
"""

CHAPTERS = [
("Getting Started",
 """<p>For the person who runs the instance. If somebody else set Legejo up and gave you an
 account, skip to Chapter 2.</p>""",
 [
 ("Running Legejo with Docker",
  "<p>You want a Legejo of your own, with as little ceremony as possible.</p>",
  """<p>Fetch the Compose file and start it:</p>
<pre>curl -O https://raw.githubusercontent.com/jularboolean/legejo/main/docker-compose.yml
docker compose up -d</pre>
<p>Open <code>http://localhost:3000</code>. A new Legejo asks its first visitor to create the
admin account: choose the language, a username and a password, and you are in, with this manual
on the first shelf.</p>""",
  """<p>Legejo is a single container. Books, covers and the database live in the
<code>legejo-data</code> volume, mounted at <code>/data</code>. That volume is your library: a
container started without it keeps everything inside itself, and loses it when the container is
removed.</p>
<p>The image is <code>slurvdjur/legejo</code> on Docker Hub, for <code>linux/amd64</code> and
<code>linux/arm64</code>, tagged with the full version, the minor version, the major version and
<code>latest</code>. A running instance idles at around 30&#160;MB of memory, so a Raspberry Pi
or the smallest VPS will do.</p>
<p>The way to create the admin closes once an account exists. Until then, anyone who reaches the
instance could take it, so create the account straight away when it is on the internet from the
start (or see the next recipe).</p>"""),

 ("Making the Admin Before the First Visit",
  "<p>You want the admin account in place before anyone can open the site.</p>",
  """<p>Set <code>LEGEJO_ADMIN_PASSWORD</code> (and <code>LEGEJO_ADMIN_USER</code>, if
<code>admin</code> does not suit you) before the first start. The account is made at the start,
and the browser shows the ordinary login.</p>""",
  """<p>Both are only read when the database is empty. Changing them later does nothing; change
the password on the <strong>Account</strong> page instead. Any setting can be read from a file by
adding <code>_FILE</code> to its name, for example
<code>LEGEJO_ADMIN_PASSWORD_FILE=/run/secrets/admin</code>, which is how Docker and Kubernetes
secrets are meant to be used.</p>"""),

 ("Using PostgreSQL Instead of SQLite",
  "<p>You already run PostgreSQL and want Legejo's data there.</p>",
  """<p>Start from <code>docker-compose.postgres.yml</code> in the repository, or set
<code>DATABASE_URL</code> to <code>postgres://user:password@host/db</code>.</p>
<p>An instance that began on SQLite can be moved:</p>
<pre>docker compose -f docker-compose.postgres.yml run --rm legejo import-sqlite /data/legejo.db</pre>""",
  """<p>SQLite needs no configuration and is the right choice for most households. The book
files stay in <code>/data</code> either way; only what Legejo knows about them moves to
PostgreSQL.</p>"""),

 ("Putting Legejo on the Internet",
  "<p>You want to reach the library from outside your home, and connect an e-reader to it.</p>",
  """<p>Put Legejo behind a reverse proxy that terminates TLS, and give the <code>https://</code>
address people reach it on under <strong>Public address</strong> on the Admin page.
<strong>Test</strong> asks the address whether it leads to this Legejo; <strong>Save</strong>
puts it to use. <code>LEGEJO_PUBLIC_URL</code> does the same, and then wins.</p>""",
  """<p>Kobo sync, links in emails, federation and installing Legejo as an app all depend on
that address being right. With an <code>https</code> public address, session cookies are marked
<code>Secure</code>; set <code>LEGEJO_SECURE_COOKIES=false</code> only if you truly serve plain
HTTP.</p>
<p>If your proxy sets a header with the client's address, name it in
<code>LEGEJO_CLIENT_IP_HEADER</code> so that login throttling (Recipe 12.7) can tell clients
apart. Only do so if the proxy always overwrites the header.</p>
<p>On Coolify, Dokploy and similar platforms: use the image <code>slurvdjur/legejo:1</code>,
route your domain to container port <code>3000</code>, and mount a persistent volume at
<code>/data</code>.</p>"""),

 ("Keeping Legejo Up to Date",
  "<p>You want to know when there is a new version, and what it changes.</p>",
  """<p>On the <strong>Admin</strong> page, under <strong>Version</strong>, press
<strong>Check for a new version</strong>. Then pull the new image and start the container
again:</p>
<pre>docker compose pull
docker compose up -d</pre>""",
  """<p>The check asks GitHub for the latest release when you press the button, and only then,
from your browser. The server itself never asks anyone anything. What changed in each version is
in the changelog in the repository and on the releases page.</p>
<p>People who have the web app open during an upgrade see a small note,
<strong>A new version of Legejo is ready</strong>, with a button to reload. The next time
anyone opens Legejo after an upgrade, a note says which version it is now, with
<strong>What is new</strong> a click away: the changes since the version that browser last
saw, read from the changelog built into Legejo itself. The Admin page has the same under
<strong>Version</strong>, for the version running.</p>"""),

 ("Backing Up",
  "<p>You want to be able to lose the machine without losing the library.</p>",
  """<p>Back up the <code>/data</code> volume. With SQLite that is everything. With PostgreSQL,
back up the database too.</p>""",
  """<p>Each user can also take their own library with them: Recipe 11.5 describes the export,
which holds the book files, the covers and the catalogue as CSV and JSON.</p>"""),
 ]),

("Filling the Library",
 "<p>A library starts empty, apart from this book. Here is how books get in.</p>",
 [
 ("Uploading Books",
  "<p>You have EPUB files on your computer and want them in Legejo.</p>",
  """<p>On <strong>My library</strong>, press <strong>Upload books</strong> and choose the
files, or drag them onto the page. Several at once is fine.</p>""",
  """<p>Title, author, series, cover and the rest are read from each file where it has them.
Every file gets a line telling you what happened to it: <em>Added</em>, <em>Already in the
library</em>, or <em>Failed</em> with the reason. The largest file accepted is 200&#160;MB unless
whoever runs the instance has changed <code>LEGEJO_MAX_UPLOAD_MB</code>.</p>"""),

 ("Understanding What Happens to a File",
  "<p>The upload says a book was repaired, or has remarks. You wonder what was done to your file.</p>",
  """<p>Every EPUB is checked on the way in. What can be mended is mended: a missing table of
contents, language, identifier or cover declaration, the marker that says the file is an EPUB,
and references to files that are not there. What cannot be mended is reported: broken links and
missing pictures. The book page shows the result under <strong>File check</strong>.</p>""",
  """<p>These are small things that make reading apps stumble: a file with no language gets the
wrong hyphenation and the wrong voice when read aloud; one with no table of contents cannot be
navigated. A book with remarks still reads as usual.</p>
<p>Copy-protected files (DRM) are refused. They can only be read in the app they were bought
for, and Legejo could do nothing with them.</p>"""),

 ("Dealing with Duplicates",
  "<p>You are not sure whether you already have a book.</p>",
  """<p>Upload it. The very same file is not added twice, and Legejo says so. A different file
that looks like the same book, by its ISBN, is added, with a note pointing at the one you
had.</p>""",
  """<p>Two editions of one work are two books, and often you want both. The note is there so
that you can decide; <strong>Dismiss</strong> it if the answer is yes.</p>"""),

 ("Letting a Folder Fill the Library",
  "<p>You would rather drop files in a folder than upload them by hand.</p>",
  """<p>The person who runs the instance sets <code>LEGEJO_IMPORT_DIR</code> to a writable
folder. EPUB, PDF and CBZ files placed there, subfolders included, are added once they have
stopped changing.</p>""",
  """<p>Afterwards each file is moved to <code>imported/</code>, <code>duplicates/</code> or
<code>failed/</code> inside the folder, so you can see what became of it. The books go to the
oldest admin unless <code>LEGEJO_IMPORT_USER</code> names someone else. The folder is looked
through every 60 seconds; <code>LEGEJO_IMPORT_INTERVAL</code> changes that, and
<code>LEGEJO_IMPORT_DELETE=true</code> deletes imported files instead of moving them.</p>
<p>This is a way in, not a mirror. Legejo takes its own copy and does not watch what you do to
the files afterwards.</p>"""),

 ("Keeping PDFs and Comics",
  "<p>Not everything you read is an EPUB.</p>",
  """<p>Upload PDF and CBZ files the same way. They sit beside the books, with covers, shelves,
tags, search and sharing.</p>""",
  """<p>They are stored exactly as they are. A PDF or a comic takes its first page as the cover,
and a comic's <code>ComicInfo.xml</code> is read for its title and series. Password-protected
PDFs are refused.</p>
<p>A PDF opens in the browser with <strong>Read</strong>: the pages one after another, fitted
to the screen. Zoom with the buttons, with two fingers, or with Ctrl and the wheel; the contents
and the search are at the top. Select text to highlight it or add a note, as in an EPUB; tap a
highlight to change or delete it.</p>
<p>A comic opens with <strong>Read</strong> as well: a page at a time fitted to the screen, or
two side by side on a wide one. Tap the right or left side, swipe, or use the arrow keys to
turn; tap the middle for the bars, where the slider goes anywhere in the comic. Double-tap, use
two fingers or Ctrl and the wheel to zoom in, and drag to move about the page. A comic whose
<code>ComicInfo.xml</code> says it is manga is read from the right. A comic also reaches a Kobo through Kobo sync, made into a book with
one page per picture; a PDF does not.</p>"""),

 ("Fetching Books from Another Library",
  "<p>You want books from Project Gutenberg, or another library with a catalog, without downloading and uploading.</p>",
  """<p><em>Optional: an admin turns it on for the instance, and you then turn it on for
yourself on the <strong>Account</strong> page.</em></p>
<p>Open <strong>Catalogs</strong>, add a catalog by its address or pick one of the suggested
ones, look through it or search it, and fetch a book into your library.</p>""",
  """<p>A fetched book is an ordinary book: checked and repaired like an uploaded one, and not
added again if you already have the file. Legejo sets no licence on it; what the catalog says
about the rights is shown with each book, and what you may do with it is yours to find out.</p>
<p>Legejo reads OPDS&#160;1 catalogs that are open to everyone. Catalogs that ask for a login,
and OPDS&#160;2, are not read.</p>"""),
 ]),

("Describing Books",
 "<p>A library is only as good as its catalogue. Legejo reads what the files say and lets you correct it.</p>",
 [
 ("Correcting What Legejo Knows About a Book",
  "<p>The title is in capitals, the author is misspelt, or the language is wrong.</p>",
  """<p>Open the book and press <strong>Edit</strong>. Change <strong>Title</strong>,
<strong>Author</strong>, <strong>Language</strong>, <strong>Publisher</strong>,
<strong>Category</strong>, <strong>ISBN</strong> or the <strong>Description</strong>, and press
<strong>Save</strong>.</p>""",
  """<p>For an EPUB, the corrections are written back into the file: title, authors, language,
series and cover. The book is then right on the e-reader and in any export, not just on this
screen. The description takes Markdown.</p>"""),

 ("Looking a Book Up",
  "<p>The file knows almost nothing about itself.</p>",
  """<p><em>Optional: an admin turns on each lookup under <strong>Metadata</strong> on the Admin
page.</em></p>
<p>On the edit page, look the book up in Open Library, or in Libris, Sweden's national library
catalogue, and take over what fits.</p>""",
  """<p>An ISBN gives the surest match. A lookup that is off has no panel on the edit
page.</p>"""),

 ("Keeping a Series in Order",
  "<p>You have seven books of a series and want them in order.</p>",
  """<p>On each book's edit page, give the same <strong>Series</strong> and its
<strong>Number in series</strong>.</p>""",
  """<p>The book page then links to the next book in the series, and the series can be opened as
a list. On a Kobo and in OPDS apps the series travels with the book.</p>"""),

 ("Tagging and Rating",
  "<p>You want your own way of grouping books, and to remember what you thought of them.</p>",
  """<p>On the edit page, write <strong>Tags</strong> separated by commas, such as
<code>mystery, 1920s, favourites</code>. Rate a book from its page.</p>""",
  """<p>A category is the one broad kind a book is (Fiction); tags are as many as you like. Both
can be filtered on (Recipe 4.2). Ratings feed the statistics on <strong>My reading</strong>.</p>"""),

 ("Telling the Work from the Edition",
  "<p>Your copy of a novel from 1879 was published in 2011, and the library sorts it among the new books.</p>",
  """<p>On the edit page, <strong>First published (year)</strong> is the year the work first
appeared. <strong>This edition</strong> is the date of your copy, as a year or a full date.</p>""",
  """<p>If you only know the edition's year and the book is recent, one button copies it to the
other field.</p>"""),

 ("Changing Many Books at Once",
  "<p>You uploaded forty books and want to tag them all, or put them on a shelf.</p>",
  """<p>On <strong>My library</strong>, press <strong>Select</strong>, mark the books, and choose
what to do with them: tag, shelve, mark as want to read, or delete.</p>""",
  "<p>Deleting removes the books and their files for good, along with their places on shelves and your place in them.</p>"),

 ("Repairing a File Later",
  "<p>A book page says <em>The file needs attention</em>.</p>",
  """<p>Read the remarks under <strong>File check</strong>. If Legejo can mend the file, press
<strong>Repair the file</strong>. If the remark is about something only you can supply, such as a
cover, follow the link to the edit page.</p>""",
  "<p>Remarks that cannot be mended, such as links that lead nowhere inside the book, stay as a note. The book reads as usual.</p>"),
 ]),

("Finding Books",
 "<p>Once there are more books than fit on a screen.</p>",
 [
 ("Searching",
  "<p>You remember a title, an author, or a phrase.</p>",
  """<p>Press <code>/</code> anywhere outside a text field, or choose <strong>Search</strong>,
and type.</p>""",
  """<p>The search covers the catalogue: titles, authors, descriptions and tags. It also finds books
on shelves that others share with you, and, under <strong>On other instances</strong>, books on
the shelves from other Legejo instances that you follow, each with a button to add it to your
library.</p>
<p>Where whoever runs the instance has set <code>LEGEJO_FULLTEXT=true</code>, tick <strong>Search
inside the books</strong> to look for words in the text of your EPUBs instead. Each book comes with
the passages that match; choose one to open the book with the search in it. Every word must occur, a
word also finds longer words it begins, and words in double quotes must stand in a row. Books on
shared shelves are searched too, and quoted, but are read only once you take them.</p>
<p>Esperanto can be typed in the x-system: <code>cxambro</code> finds <em>ĉambro</em>, in the
catalogue and in the text alike.</p>"""),

 ("Filtering and Sorting",
  "<p>You want the unread crime novels in French, newest first.</p>",
  """<p>On <strong>My library</strong>, open <strong>Filter</strong> and choose; pick the order
beside it. The two buttons next to it switch between covers and a list.</p>""",
  "<p>The filter by title or author above the books narrows what is shown as you type.</p>"),

 ("Browsing by Author",
  "<p>You want to see everything by one writer.</p>",
  "<p>Open <strong>Authors</strong>, or click an author's name on any book page.</p>",
  """<p>A book with several authors appears under each of them.</p>
<p><em>Optional: an admin turns on <strong>Authors from Wikidata</strong>.</em> Then a line
beside the author's name says who the person was: when they lived, a few words about them, and a
link to Wikipedia in your language. It appears only when Wikidata knows the person for certain,
so a writer with a common name, or one Wikidata does not know, gets none. If the line is about
someone else, choose <strong>Wrong person?</strong>; the line is gone for you, and
<strong>Show again</strong> brings it back.</p>"""),

 ("Asking the Librarian",
  "<p>You do not know what you want to read, only what kind of thing.</p>",
  """<p><em>Optional: whoever runs the instance sets it up, and you turn it on for yourself on
the <strong>Account</strong> page.</em></p>
<p>Open <strong>The librarian</strong> and ask in your own words: <em>something short and funny
I have not read</em>, or <em>what should I read after The Terror?</em></p>""",
  """<p>The librarian is a language model. What is sent to it is your question and what the
catalogue says about the books: title, author, shelves, tags, whether you have read them, and the
beginning of the description. The books themselves are never sent. It can only answer with books
from your library and from shelves shared with you, and nothing it writes is shown except the
choice of books. You can ask 30 questions an hour.</p>"""),
 ]),

("Shelves and Sharing",
 "<p>Shelves are how you arrange a library, and how you let others into part of it.</p>",
 [
 ("Making a Shelf",
  "<p>You want your summer reading in one place.</p>",
  """<p>On a book's edit page, under <strong>Shelves</strong>, type a name in
<strong>New shelf…</strong> and press <strong>Add</strong>. Or select several books in the library
and shelve them together (Recipe 3.6).</p>""",
  """<p>A book can stand on any number of shelves. Deleting a shelf removes the shelf; the books
stay in your library. On a Kobo, shelves arrive as collections.</p>"""),

 ("Sharing a Shelf",
  "<p>You want your family to be able to read what you have collected.</p>",
  """<p>Open the shelf, press <strong>Edit</strong>, and choose who sees it: only you
(<em>private</em>), the users you pick (<em>restricted</em>), or everyone on the instance
(<em>shared with everyone</em>).</p>""",
  """<p>Others find it under <strong>Shared shelves</strong>, as covers or as a list with the
shelf's description. Give a shared shelf a description; it is what tells people whether to look
inside.</p>"""),

 ("Taking a Book from a Shared Shelf",
  "<p>Someone has shared a shelf with a book you want to read.</p>",
  "<p>Open the book on the shared shelf and copy it into your own library.</p>",
  """<p>The copy is yours: your place in it, your tags, your shelves. It takes no extra room on
the disk, since both copies point at the same file until one of them is changed.</p>"""),

 ("Following a Shared Shelf",
  "<p>A friend keeps adding books to a shelf they share with you, and you would like to know when, without going to look.</p>",
  """<p>Open the shelf under <strong>Shared shelves</strong> and press <strong>Follow</strong>.
From then on, books that come onto it are announced above your library, under <strong>New on
shelves you follow</strong>, and their number shows beside <strong>Shared shelves</strong> in
the sidebar. On the shelf itself the new ones are marked <strong>New</strong>.</p>
<p>Opening the shelf counts as having seen them; <strong>Mark as seen</strong> above the
library clears them all at once. <strong>Stop following</strong> is on the shelf.</p>""",
  """<p>A shelf you follow also has an <strong>Atom feed</strong>, an address for a feed reader
that shows each book as it arrives. The address is yours alone and needs no login, so keep it
to yourself. It stops working when you stop following, or when the shelf is no longer shared
with you. Shelves you follow from other instances (the next recipes) are not announced this
way.</p>"""),

 ("Giving a Shelf to Someone on Another Instance",
  "<p>A friend has a Legejo of their own, or reads in an app, and you want them to have one of your shelves. Not the whole fediverse; just them.</p>",
  """<p><em>Optional: an admin turns on <strong>Shelves by private link</strong> on the Admin
page.</em></p>
<p>Open the shelf, press <strong>Edit</strong>, and go to <strong>Private links</strong>. Say
who the link is for, choose how long it works, and press <strong>Make a link</strong>. It is
copied for you; send it to them.</p>""",
  """<p>The link is an address with a long secret in it. Whoever holds it gets the shelf, its
covers and its books without an account on your instance and without a password, so send it the
way you would hand over a key, and make one for each person. Then one can be taken back without
disturbing the others: press the cross beside it, and it stops working at once. Books already
fetched with it stay where they are.</p>
<p>The list shows when each link was last used, which is how you notice one that has ended up
somewhere it should not. A link can be given a last day: a week, a month or a year.</p>
<p>The link is an ordinary OPDS catalog, so it also works in KOReader, Thorium and other reading
apps, added as a catalog there. Your ratings do not travel with the books.</p>
<p>This is for people you know. What you may share with them is yours to judge.</p>"""),

 ("Following a Shelf You Were Sent a Link To",
  "<p>Someone has sent you a link to a shelf of theirs.</p>",
  """<p><em>Optional: the same admin setting as in the recipe before.</em></p>
<p>Open <strong>Shared shelves</strong>, and at the bottom of the page press <strong>Follow a
shelf from another Legejo instance</strong>. Paste the link and press <strong>Follow</strong>. The shelf opens with its books; press
<strong>Add to my library</strong> on the ones you want. Press a book's cover or title for a
closer look: publisher, dates, language and the whole description.</p>""",
  """<p>The shelf is read from its link when you open it, so it is as its owner has it,
give or take a few minutes. A book you add is a copy of your own, checked and repaired like an uploaded one, and it
stays yours whatever happens to the link.</p>
<p>From then on the shelf is listed there, under <strong>Shelves from other Legejo
instances</strong>. The list marks a shelf whose instance is not answering, and one whose link no longer works,
which is what you see when the owner has taken it back. Nothing is removed for you:
<strong>Stop following</strong> takes a shelf off your list when you have had enough of it.</p>
<p>Your Legejo keeps the link's address to itself: it is not shown again, and covers and books
are fetched by the server, never by your browser.</p>"""),

 ("Federating a Shelf of Free Books",
  "<p>You have made a fine shelf of public-domain classics and want people on other instances to enjoy it.</p>",
  """<p><em>Optional: an admin turns federation on and decides which instances are allowed.</em></p>
<p>Make the shelf <em>federated</em>. It gets an address of the form
<code>@shelf@your.instance</code>, which others can follow from another Legejo, with <strong>Follow a shelf</strong>, or from Mastodon.</p>""",
  """<p>Only free books travel this way: public domain or Creative Commons, with a source given.
A book whose file claims copyright is marked as protected from the start and stays home.</p>
<p>A follow to or from an instance that is not yet allowed waits for the admin, who sees it as a
request. A followed shelf whose instance stops answering is marked, and dropped after a week.</p>
<p>On the Fediverse page, press a book's cover or title for what its instance knows about it:
series, when it was first published, publisher, license, the whole description, and a link to the
book's page there.</p>
<p>To begin with, the Fediverse page suggests a couple of shelves worth following, among them five
Esperanto classics, and the federation tab of the Admin page offers their instance beside
<strong>Connect an instance</strong>. <code>LEGEJO_SHELF_SUGGESTIONS=off</code> hides them.</p>"""),
 ]),

("Reading in the Browser",
 "<p>The web reader opens EPUB books. Everything in this chapter happens there.</p>",
 [
 ("Opening a Book and Turning Pages",
  "<p>You want to read.</p>",
  """<p>Open the book and press <strong>Read</strong>, or <strong>Continue reading</strong> if
you have begun. Click or tap the right third of the page to go forward and the left third to go
back. The middle shows or hides the toolbars. Swiping turns pages too, and so do the arrow keys,
Space and Page Down.</p>""",
  """<p>Your place is saved as you read and sent to the server, so the book opens where you left
it on any device. If you last read it on a Kobo or in KOReader, the reader starts at about the
same place: those devices report a percentage, not a line. The other way is exact: a Kobo opens
the book at the sentence you reached in the browser, after its next sync (Recipe 8.2).</p>"""),

 ("Making the Page Yours",
  "<p>The text is too small, the lines too tight, or the white too bright.</p>",
  """<p>Open <strong>Reading settings</strong> (the <strong>Aa</strong> button, or the
<code>S</code> key). Choose a <strong>Theme</strong> (Light, Sepia or Dark), the
<strong>Text size</strong>, a <strong>Typeface</strong> (the book's own, Serif or Sans),
<strong>Line spacing</strong>, <strong>Margins</strong> and <strong>Alignment</strong>.</p>""",
  """<p><strong>Hyphenation</strong> breaks long words at line ends, in the language the book
says it is in, which is one reason Recipe 2.2 cares about languages. <strong>Page animation</strong>
turns the page like a leaf of a book: on a phone it follows your finger as you pull the page
aside, and two pages side by side are drawn away like a sheet. The <code>+</code> and <code>−</code> keys change the
text size without opening the panel. The settings are kept on the device you set them on.</p>"""),

 ("Finding Your Way in a Book",
  "<p>You want Chapter 9, or the place where a name first appears.</p>",
  """<p>The <strong>Table of contents</strong> (the <code>T</code> key) lists the chapters.
<strong>Search in the book</strong> (the <code>/</code> key) finds a word or phrase and takes you
to each place it stands.</p>""",
  "<p>The line at the bottom shows where you are in the chapter and in the book; drag it to move.</p>"),

 ("Having a Book Read Aloud",
  "<p>Your eyes are tired, or your hands are busy.</p>",
  """<p>Press <strong>Read aloud</strong> (the <code>L</code> key). The sentence being read is
marked, and the pages turn along. Step a sentence back or forward, change the
<strong>Speed</strong>, and choose a <strong>Voice</strong> among those your device has.</p>""",
  """<p>The voices are your device's own; nothing is sent anywhere. If the device has no voice
for the book's language, another one reads, and says so. Once more, the book's language matters.</p>"""),

 ("Highlighting and Making Notes",
  "<p>You want to keep a passage, and what you thought of it.</p>",
  """<p>Select the passage. A bar appears at the bottom: press <strong>Highlight</strong>, or
<strong>Add a note</strong>, write, and <strong>Save</strong>.</p>
<p>Where text cannot be selected, which is Safari and Legejo as an app on iPhone, iPad and Mac:
drag across the passage with the mouse, or hold a finger on a word. Drag on without lifting
the finger, or drag the two handles afterwards, to take in more words; <strong>Whole sentence</strong>
takes the sentence around them.</p>""",
  """<p>A tap on a highlight opens it, to read the note, change it or delete it. With a mouse,
resting the pointer on a highlight shows its note beside it. The book page says how many
highlights and notes a book has.</p>"""),

 ("Going Through Your Notes",
  "<p>You have finished the book and want what you marked.</p>",
  """<p>Open <strong>Highlights and notes</strong> (the <code>N</code> key). Each entry shows
the passage and the note; a click takes you there. The download button at the top of the panel
gives you all of them as a Markdown file, and so does <strong>Export as Markdown</strong> on the
book page.</p>
<p>For every book at once, open <strong>My notes</strong> in the sidebar. The highlights stand
book by book, in the order they come in the text, each with its chapter and how far through the
book it is, and the page of the printed edition when the book has one. Search them, write or
change a note right there, or choose <strong>Show in the book</strong> to open the reader at the
passage; your place in the book stays where it was. <strong>Export all</strong> gives every
book's notes in one file.</p>""",
  "<p>Markdown opens in any note-taking app and any text editor. Notes made on a Kobo are on the list too, marked Kobo.</p>"),

 ("Reading with the Keyboard",
  "<p>You prefer keys to the mouse.</p>",
  """<p>Press <code>?</code> for the list. In short:</p>
<table>
<tr><th>Key</th><th>What it does</th></tr>
<tr><td><code>→</code>, <code>Space</code>, <code>PgDn</code></td><td>Next page</td></tr>
<tr><td><code>←</code>, <code>Shift</code>+<code>Space</code>, <code>PgUp</code></td><td>Previous page</td></tr>
<tr><td><code>T</code></td><td>Table of contents</td></tr>
<tr><td><code>/</code></td><td>Search in the book</td></tr>
<tr><td><code>S</code></td><td>Reading settings</td></tr>
<tr><td><code>N</code></td><td>Highlights and notes</td></tr>
<tr><td><code>L</code></td><td>Read aloud, pause</td></tr>
<tr><td><code>+</code>, <code>−</code></td><td>Larger or smaller text</td></tr>
<tr><td><code>F</code></td><td>Full screen</td></tr>
<tr><td><code>Esc</code></td><td>Close a panel; show or hide the toolbars</td></tr>
<tr><td><code>?</code></td><td>This list</td></tr>
</table>""",
  None),

 ("Looking a Word Up",
  "<p>You are reading in a language you are still learning, and a word stops you.</p>",
  """<p><em>Optional: an admin turns on <strong>Look words up while reading</strong> on the Admin
page.</em></p>
<p>Select the word and choose <strong>Look up</strong>. In a book in Esperanto the word comes from
Reta Vortaro, the open Esperanto dictionary, with what it is in your own language and in English;
in other languages it comes from Wiktionary, with what it means. The headword links to the whole
article.</p>""",
  """<p>A word is looked up as the dictionary has it: <em>ĉambrojn</em> as <em>ĉambro</em>,
<em>legis</em> as <em>legi</em>. The server asks the dictionary for you, so the words you look up
leave the instance, but not who you are.</p>"""),
 ]),

("Legejo as an App",
 """<p>Legejo can be put on the home screen of a phone or tablet, or in the Dock of a Mac, where
it opens in a window of its own. There, books can be saved for reading without the network.</p>""",
 [
 ("Putting Legejo on Your Home Screen",
  "<p>You want Legejo among your apps, not in a browser tab.</p>",
  """<p><strong>iPhone and iPad:</strong> open Legejo in Safari, press the share button, and
choose <strong>Add to Home Screen</strong>.</p>
<p><strong>Mac:</strong> in Safari, <strong>File</strong>, then <strong>Add to Dock</strong>.</p>
<p><strong>Android, and Chrome or Edge on a computer:</strong> choose <strong>Install</strong>
from the browser's menu.</p>""",
  """<p>It is the same Legejo, from the same server; nothing is fetched from an app store. It
needs the instance to be served over <code>https</code> (Recipe 1.4).</p>
<p>On a phone, the tabs at the foot of the screen take you to the library, your reading and the search; <strong>Shelves</strong> and <strong>More</strong> open the rest from below.</p>"""),

 ("Saving a Book on the Device",
  "<p>You are about to board a plane.</p>",
  """<p>Open the book and press <strong>Save on this device</strong>. When it says
<strong>On this device</strong>, the book is there. Press that button again to remove it.</p>""",
  """<p>On an iPhone, an iPad, and in Safari on a Mac, the button is only offered once Legejo is
on the home screen or in the Dock. In a browser tab Safari empties what a site has stored after a
week without a visit, and a book that quietly disappears is worse than no book. The page tells
you so instead of offering the button.</p>
<p><strong>On this device</strong> in the menu lists what is saved and how much room it takes.
Logging out removes it all.</p>"""),

 ("Reading Without the Network",
  "<p>You are on that plane.</p>",
  """<p>Open Legejo. It starts from what is on the device and takes you to
<strong>On this device</strong>, where the saved books are. Open one and read.</p>""",
  """<p>Your place is kept on the device and sent to the server when it can be reached again. If
you read further in the same book on another device in the meantime, the later place wins.</p>
<p>Without the network you can read your earlier highlights but not make new ones, and the rest
of the library cannot be browsed. <strong>Try again</strong> asks the server once more.</p>"""),

 ("When a New Version Is Ready",
  "<p>A small note says <em>A new version of Legejo is ready</em>.</p>",
  "<p>Press <strong>Reload</strong>. That is all.</p>",
  """<p>An app on the home screen has no reload button of its own, which is why Legejo offers
one. The note appears when the server has been upgraded while you had the app open.</p>"""),
 ]),

("E-readers and Reading Apps",
 "<p>Legejo is made for e-readers. Sync with them is part of it, not an add-on.</p>",
 [
 ("Connecting a Kobo",
  "<p>You want your library on your Kobo, without cables, each time it syncs.</p>",
  """<ol>
<li>On the <strong>Account</strong> page, under <strong>Kobo sync</strong>, press
<strong>Generate token</strong>, then <strong>Copy URL</strong>.</li>
<li>Connect the Kobo to a computer with its cable.</li>
<li>Open the file <code>.kobo/Kobo/Kobo eReader.conf</code> on the device in a text editor. The
<code>.kobo</code> folder is hidden.</li>
<li>Under <code>[OneStoreServices]</code>, set <code>api_endpoint=</code> to the address you
copied.</li>
<li>Eject the Kobo and sync it.</li>
</ol>""",
  """<p>Legejo then stands where the Kobo store stood: the device asks Legejo what books there
are. The address holds your token, which is what tells Legejo whose library to send. Keep it to
yourself; <strong>Regenerate</strong> makes a new one and the old one stops working.</p>
<p><strong>The Kobo store no longer works on the device</strong> while it syncs with Legejo,
unless an admin has turned on <strong>The Kobo store beside Legejo</strong> (Recipe 8.4):
browsing, searching, deals and the wishlist come up empty, and books bought from Kobo, or
borrowed through Kobo Plus, do not arrive. Books already on the device stay, and your Kobo
account stays signed in. To shop on the device again, set <code>api_endpoint=</code> back to
<code>https://storeapi.kobo.com</code>.</p>
<p>The instance must be reachable from the Kobo at the address in
<code>LEGEJO_PUBLIC_URL</code> (Recipe 1.4).</p>
<p>Once the device has synced, the same place on the Account page lists it under
<strong>Your Kobo devices</strong>, with when it was last seen, and says how the last sync
went: what was sent, or that it failed and with what. That is where to look first when
something does not arrive on the device.</p>"""),

 ("Knowing What Syncs with a Kobo",
  "<p>You wonder what will show up, and what will come back.</p>",
  """<p>Your EPUB books arrive, converted on the way to Kobo's own format so that the device's
reader works fully. Shelves arrive as collections. Your place in each book goes both ways, to the sentence: read
on the Kobo, and the browser opens the book at the line you stopped at; read in the browser, and
the Kobo opens the book at that sentence once it has synced. The time the Kobo counts reading a
book shows on the book's page. So does how long a book takes to read, at a common pace until the
web reader or the Kobo has timed books you finished, and from then on at yours; the web reader says about how long
is left of the chapter and of the book. Comics arrive as books with one page per picture.</p>""",
  """<p>PDF files do not sync. A book counts as finished at 99&#160;% read, on the Kobo as in the
web reader. A place read in the browser reaches a book the Kobo has downloaded; a book still
waiting to be downloaded gets it with the download. Removing a book on the Kobo removes it from
the device, not from your library; its page in Legejo can send it to the device again.</p>"""),

 ("Syncing Highlights with a Kobo",
  "<p>You highlight on the Kobo and want those passages in Legejo, and the other way round.</p>",
  """<p><em>Optional: an admin turns on <strong>Highlights from Kobo e-readers</strong> on the
Admin page.</em></p>
<p>Sync the Kobo once after it has been turned on. From then on, what you highlight and note on
the device lands in Legejo, placed in the book and marked <em>Kobo</em> in the panel. What you
highlight in the web reader, and notes you change there, reach the device at its next sync.
Removing one removes it on the other side.</p>""",
  """<p>Know these things before turning it on:</p>
<ul>
<li>The device then sends its highlights to Legejo instead of to Kobo, for all its books. Those
of books bought from Kobo are not taken in. With <strong>The Kobo store beside Legejo</strong>
on as well (Recipe 8.4), Legejo passes them on to Kobo as before; without it they no longer
reach Kobo, and if your device holds such books, leave the setting off.</li>
<li>Highlights already on the device before the setting was turned on are not sent, unless you
change them on the device.</li>
<li>It has been tried on one device, a Kobo Clara&#160;2E.</li>
</ul>"""),

 ("Keeping the Kobo Store",
  "<p>You buy books from Kobo too, and want them on the device beside your own.</p>",
  """<p><em>Optional: an admin turns on <strong>The Kobo store beside Legejo</strong> on the
Admin page.</em></p>
<p>Sync the Kobo. The store works on the device again, and the books you bought, or borrowed
through Kobo Plus, arrive beside the ones from Legejo, with their places and highlights kept
by Kobo as before.</p>""",
  """<p>The device keeps its own Kobo account. Legejo answers for its own books and passes what
concerns Kobo's on to Kobo: Legejo's server talks to Kobo's on your behalf, with the device's
sign-in. Nothing of it is kept in Legejo, apart from where Kobo's sync has come to.</p>
<p>If Kobo cannot be reached, Legejo's books still sync, and Kobo's come at a later sync. The
highlights of a bought book are never answered as "none" in the meantime, so the device keeps
them.</p>
<p>Bought books are not in your Legejo library: they cannot be read in the browser, and shelves
do not take them.</p>"""),

 ("Reading in KOReader",
  "<p>You read in KOReader, on an Onyx Boox, a PocketBook, a Kindle or a Kobo.</p>",
  """<p>Get the books through OPDS. On the <strong>Account</strong> page, under
<strong>OPDS catalog</strong>, is the address to add as a catalog in KOReader. Log in with your
username and an app password (Recipe 11.2).</p>""",
  """<p>The catalog lists your books, your shelves and your series, and can be searched.</p>
<p>KOReader on a Kobo can also read the books that Kobo sync put on the device. Legejo
knows those files too, so the reading position of such a book finds its way here as well
(Recipe 8.6).</p>"""),

 ("Syncing Your Place with KOReader",
  "<p>You want the place you reached in KOReader to show in Legejo, and follow you between KOReader devices.</p>",
  """<ol>
<li>On the <strong>Account</strong> page, under <strong>KOReader sync</strong>, press
<strong>Create sync key</strong>.</li>
<li>In KOReader, open a book, tap the tools icon and choose <strong>Progress sync</strong>.</li>
<li>Choose <strong>Custom sync server</strong> and enter the server address shown on the Account
page.</li>
<li>Choose <strong>Login</strong> (not Register) and enter your username, and the sync key as
the password.</li>
<li>Turn on automatic sync if you want the place sent while you read.</li>
</ol>""",
  """<p>Books are recognised by their file, so they must come from Legejo, by OPDS or by
download. In a book, Legejo shows the place as a percentage; the web reader then starts at about
the same place, not the exact line. A PDF or a comic goes by page, both ways: the web reader opens
at the page you reached in KOReader, and KOReader is offered the page you reached in the browser
when it is newer.</p>"""),

 ("Using Other Reading Apps",
  "<p>You read in Moon+ Reader, Thorium, or something else that speaks OPDS.</p>",
  """<p>Add the address from <strong>OPDS catalog</strong> on the Account page as a catalog in
the app, and log in with your username and an app password.</p>""",
  """<p>Legejo speaks OPDS&#160;1.2. EPUB, PDF and CBZ files are all handed out this way, exactly
as they are stored.</p>"""),

 ("Sending a Book to a Kindle",
  "<p>You have a Kindle, which takes its books by mail.</p>",
  """<p>On the <strong>Account</strong> page, under <strong>Send to Kindle</strong>, give the
address of your Kindle. It ends in <code>@kindle.com</code>, and Amazon shows it in your account.
Then press <strong>Send to Kindle</strong> on a book's page.</p>""",
  """<p>The instance must have mail set up (Recipe 12.2), and Amazon must have the instance's
sender address on your list of approved senders, or it will silently drop the book. Comics
cannot be sent.</p>"""),
 ]),

("Keeping Track of Your Reading",
 "<p>What you are reading, what you mean to read, and what you have read.</p>",
 [
 ("Seeing Where You Are",
  "<p>You are in the middle of four books and have lost track.</p>",
  """<p>Open <strong>My reading</strong>. At the top are the books in progress, with how far you
have come, ready to continue.</p>""",
  "<p>Your place comes from wherever you last read: the web reader, a Kobo or KOReader.</p>"),

 ("Keeping a List of What to Read Next",
  "<p>You keep meaning to read a book and keep forgetting which.</p>",
  """<p>Press <strong>Want to read</strong> on the book's page, or select several books in the
library and mark them together. They are listed on <strong>My reading</strong>.</p>""",
  "<p>Press the button again to take a book off the list.</p>"),

 ("Looking Back",
  "<p>You wonder how much you read last year, and what.</p>",
  """<p>Further down <strong>My reading</strong> are <strong>Recently finished</strong> and
<strong>Statistics</strong>: books finished per month, books added per month, the authors you
have read most, the languages, and your ratings. With a Kobo, the time it has counted reading is
there too, added up over the books.</p>""",
  """<p>A book counts as finished at 99&#160;% read, or when you mark it so (the next recipe).
Its month is the one in which it was last read, or marked. Nothing here leaves your
instance.</p>"""),

 ("Marking a Book as Read, or Starting Over",
  "<p>You read a book on paper, or its last pages are notes, so Legejo never counts it as finished. Or you opened a book by mistake and do not want it among the ones you are reading.</p>",
  """<p>On the book's page, press <strong>Mark as read</strong>. The book counts as finished from
then on, whatever the position says.</p>
<p>For a book opened by mistake, press <strong>Start over</strong>; for a finished one,
<strong>Mark as unread</strong>. Both forget your place in the book and put it back among the
unread.</p>""",
  """<p>Starting over reaches the devices too. A Kobo is handed a fresh state at its next sync and
takes it, since it is newer than its own. KOReader keeps its own place in the book until you
open the book there again, at which it sends that place back; start over in KOReader as well
if you mean it.</p>
<p>Marking as read also tells a Kobo the book is finished. And it works the other way: a
book marked as unread on the Kobo is unread here after the next sync, and a place read in the
web before that is forgotten. The book itself is not touched by any of this.</p>"""),
 ("Taking Your Reading Elsewhere",
  "<p>You keep track of your reading on StoryGraph, BookWyrm or Goodreads too, and do not want to enter it all again.</p>",
  """<p>On the Account page, under <strong>Export your library</strong>, press <strong>Reading as
CSV (Goodreads)</strong>. Import the file where you want your reading: StoryGraph, BookWyrm and
Goodreads all take a file in Goodreads' own export format, which this is.</p>""",
  """<p>The file holds the books you have read, are reading or want to read, with your ratings,
the day you finished (the day you marked it read, or last read in it), the day you added it,
and your shelves. Books you have not touched are left out. The other service matches books on
ISBN, title and author, so a book without an ISBN may need a hand there.</p>"""),
 ]),

("Audiobooks",
 """<p>An optional extra, and deliberately a small one. Legejo stores the files and serves them
as a podcast feed; it has no listening app of its own.</p>""",
 [
 ("Turning Audiobooks On",
  "<p>You want audiobooks beside your books.</p>",
  """<p>An admin turns on <strong>Audiobook support</strong> on the Admin page.
<strong>Audiobooks</strong> then appears in the menu.</p>""",
  "<p>It is off by default because audio files take far more room than books.</p>"),

 ("Adding an Audiobook",
  "<p>You have a folder of MP3 files, or one M4B.</p>",
  """<p>On <strong>Audiobooks</strong>, upload the MP3, M4A or M4B files, or the whole folder.
Title, author, narrator, cover and length are read from the files.</p>""",
  """<p>Each file is one part, played in the order of the file names. Whoever runs the instance
can also set <code>LEGEJO_AUDIOBOOK_IMPORT_DIR</code>: each folder placed at the top of it
becomes one audiobook, and an image beside the files becomes the cover. The audio is never held
in memory, so audiobooks of several gigabytes are fine.</p>"""),

 ("Listening",
  "<p>You want to listen on your phone, and have it remember where you were.</p>",
  """<p>Open the audiobook. It has a private podcast feed, and buttons that open it in Apple
Podcasts, Pocket Casts, Overcast and Castro. Any other podcast app takes the feed's address.</p>""",
  """<p>The podcast app keeps your place, the way it does for any show. That is the whole idea:
you already have an app that is good at this.</p>"""),

 ("Sharing an Audiobook",
  "<p>The rest of the household wants to hear it too.</p>",
  "<p>Share the audiobook with the users you choose, or with everyone on the instance.</p>",
  """<p>Every listener has a feed address of their own. When the sharing ends, their address
stops working, and nobody else's is affected.</p>"""),
 ]),

("Your Account",
 "<p>Everything in this chapter is on the <strong>Account</strong> page, reached by the cog beside your name.</p>",
 [
 ("Changing Your Password, Picture and Language",
  "<p>You were given a password you did not choose, and the interface is in the wrong language.</p>",
  """<p>Set a <strong>New password</strong> (at least 8 characters); your
<strong>Current password</strong> is needed to save. Give an <strong>Email</strong> address,
for resetting your password. Choose a <strong>Profile picture</strong>.
Choose a <strong>Language</strong>: English, Swedish, Finnish, German, French, Spanish or
Esperanto.</p>""",
  "<p>Where the instance sends mail, a new email address is used once you open the link sent to it; until then the old one stays, and it is told of the change. In English or Esperanto, <strong>Add the manual in this language to my library</strong> puts this book in that language on your shelves. The language takes effect at once, for your account only. A small thing worth knowing: a link to a book or a shelf that asks you to log in takes you to that book or shelf once you have, not to the start page.</p>"),

 ("Giving Each App a Password of Its Own",
  "<p>You would rather not type your real password into a reading app.</p>",
  """<p>Under <strong>App passwords</strong>, give a name, such as <em>KOReader on the Boox</em>,
and press <strong>Create</strong>. Copy the password at once: it is shown only once.</p>""",
  """<p>Hyphens and capitals in it do not matter, which helps when typing on an e-reader. The
list shows when each was last used. Remove one, and the app using it can no longer log in,
without anything else being disturbed. Your normal password still works everywhere.</p>"""),

 ("Letting an AI Assistant Read Your Library",
  "<p>You use an AI assistant and want to ask it about your own books.</p>",
  """<p><em>Optional: whoever runs the instance sets <code>LEGEJO_MCP=true</code>.</em></p>
<p>Create an app password and give it to the assistant as a bearer token for the address
<code>/api/mcp</code> on your instance. In Claude Code, for example:</p>
<pre>claude mcp add --transport http legejo https://books.example.org/api/mcp \\
  --header "Authorization: Bearer &lt;app password&gt;"</pre>""",
  """<p>This is a Model Context Protocol server, and it is read-only. The assistant can search
your library, look at a book, your shelves and your reading, read a book's table of contents and
its sections, search inside a book, and see the shelves others share with you. Where the instance
keeps the text of the books (<code>LEGEJO_FULLTEXT</code>), it can also search the text of them all
at once. It sees your
library and nothing else, and the text of a book only leaves the server when the assistant asks
for it.</p>
<p>Clients that keep the <code>Authorization</code> header for themselves can send the app
password in an <code>X-Api-Key</code> header instead.</p>"""),

 ("Choosing How Legejo Looks",
  "<p>The greys are not for you, or you want dark in daylight, or a little movement on the page.</p>",
  """<p>On the <strong>Account</strong> page, open <strong>Design</strong> and pick one:
<strong>Automatic</strong> (light or dark, as your system is set), <strong>Light</strong>,
<strong>Dark</strong>, or <strong>Zamenhof</strong>, in the browns of an old textbook with the
green star of Esperanto and a book typeface. Below it, <strong>Show animations</strong> lets
menus, dialogs, covers and pages come in softly instead of all at once.</p>""",
  """<p>Both choices are kept on the device, so a phone and a computer can differ. Animations
are off until you turn them on. The reader has themes of its own, in its
<strong>Reading settings</strong> (Recipe 6.2), and is not changed by the design.</p>"""),

 ("Exporting Your Library",
  "<p>You want a copy of everything, or you are leaving.</p>",
  """<p>On the Account page, under <strong>Export your library</strong>, ask for an export. It
holds your book files, the covers, and the catalogue as CSV and JSON. Your account can be
deleted from the same page. For your reading alone, in a form other services take in, see
Recipe 9.5.</p>""",
  """<p>Because corrections are written back into EPUB files (Recipe 3.1), the exported books
carry your work with them. Deleting an account removes its books for good.</p>"""),
 ]),

("Running an Instance",
 "<p>For the person who runs it. Most of this is on the <strong>Admin</strong> page; the rest is environment variables.</p>",
 [
 ("Letting People In",
  "<p>You want to give your family accounts.</p>",
  """<p>On the Admin page, under <strong>Users</strong>, create an invitation and send the link.
Or turn on <strong>Allow users to register themselves</strong>.</p>""",
  """<p>Self-registration requires mail to be set up, since new accounts must confirm their email
address. Any user can be made an admin.</p>
<p>An account can go too: <strong>Delete account</strong> beside a user removes it with its books,
shelves, highlights, notes and reading, for good. Your own account you delete on the account
page.</p>"""),

 ("Setting Up Mail",
  "<p>Invitations, password resets and Send to Kindle all need to send mail.</p>",
  """<p>On the Admin page, under <strong>Email</strong>, fill in the server, port, security,
username, password and sender your mail provider gives you. <strong>Send a test mail</strong>
tries them before you save, to your own address or another; <strong>Save</strong> puts them to
use at once.</p>""",
  """<p>The same can be set with <code>LEGEJO_SMTP_HOST</code> and <code>LEGEJO_MAIL_FROM</code>,
and as needed <code>LEGEJO_SMTP_PORT</code> (587), <code>LEGEJO_SMTP_USER</code>,
<code>LEGEJO_SMTP_PASSWORD</code> and <code>LEGEJO_SMTP_TLS</code> (<code>starttls</code>,
<code>tls</code> or <code>none</code>); then the Admin page shows them and cannot change them.
The password saved on the Admin page is kept in the database.</p>
<p>Mail is off when no server is given, and everything that depends on it is hidden. A
forgotten password can still be put right without it (Recipe 12.8).</p>"""),

 ("Signing In with Single Sign-On",
  "<p>Your household already has one login for everything.</p>",
  """<p>Set <code>LEGEJO_OIDC_ISSUER</code>, <code>LEGEJO_OIDC_CLIENT_ID</code> and
<code>LEGEJO_OIDC_CLIENT_SECRET</code>, and register
<code>&lt;LEGEJO_PUBLIC_URL&gt;/api/auth/oidc/callback</code> as the redirect address at the
provider.</p>""",
  """<p>Any OpenID Connect provider works: Pocket ID, Authentik, Keycloak, Forgejo. ID tokens
must be signed with RS256. Password login keeps working beside it.
<code>LEGEJO_OIDC_NAME</code> is the label on the button; <code>LEGEJO_OIDC_AUTO_CREATE=true</code>
creates accounts for people not seen before; <code>LEGEJO_OIDC_ADMIN_GROUP</code> makes members
of a group admins. Someone signing in for the first time is matched to an account with the same
verified email address.</p>"""),

 ("Choosing What the Instance Offers",
  "<p>You want to know what can be switched on and off.</p>",
  """<p>On the Admin page: the Libris and Open Library lookups, self-registration, audiobook
support, looking words up while reading, catalogs of other libraries, highlights from Kobo
e-readers, the Kobo store beside Legejo, shelves by private link, and federation with its list of allowed and blocked instances.
The librarian's model is set up there too, with its API address, model and key, and a
<strong>Test</strong> that asks it one short question.</p>
<p>By environment variable: the server for AI assistants (<code>LEGEJO_MCP</code>), and the
librarian if you prefer (<code>LEGEJO_AI_API_KEY</code>, with <code>LEGEJO_AI_BASE_URL</code> and
<code>LEGEJO_AI_MODEL</code>).</p>""",
  """<p>The librarian works with any provider that has an OpenAI-style chat API, a model you run
yourself included. You pay for the requests: a question sends the catalogue of that user's books,
so its cost grows with the library. A few hundred books take some twenty thousand tokens. The
Admin page shows how many questions were asked and how many tokens they took.</p>
<p>Without these two, nothing in Legejo talks to a language model.</p>"""),

 ("Seeing What Has Happened",
  "<p>Something is odd, and you want to know what the instance has been doing.</p>",
  "<p>The Admin page has a system log of everything that happens on the instance: logins, uploads, syncs, shares, failures.</p>",
  """<p>For the server's own log, <code>LEGEJO_LOG_FORMAT=json</code> gives one JSON object per
line, and <code>RUST_LOG</code> sets the level.</p>"""),

 ("Measuring",
  "<p>You run Prometheus and want Legejo in it.</p>",
  """<p>Set <code>LEGEJO_METRICS_TOKEN</code>; <code>/metrics</code> then answers requests that
carry <code>Authorization: Bearer &lt;token&gt;</code>.</p>""",
  """<p>It reports users, books, storage, shelves, the federation queue, exports, failed logins
and activity per kind of event. <code>LEGEJO_METRICS=true</code> opens it without a token, which
is for private networks only.</p>"""),

 ("Keeping Password Guessers Out",
  "<p>Your instance is on the internet, and somebody is trying passwords.</p>",
  "<p>Nothing to do: failed logins are counted, on the web and over OPDS, and answered with HTTP&#160;429 once the limit is reached within the window.</p>",
  """<p>The limit is 10 failures per username in 15 minutes, and 50 per client address across
usernames. <code>LEGEJO_LOGIN_MAX_FAILURES</code>, <code>LEGEJO_LOGIN_MAX_FAILURES_IP</code> and
<code>LEGEJO_LOGIN_WINDOW_MINUTES</code> change them. Behind a reverse proxy, name the header
that carries the client's address (Recipe 1.4), or every visitor looks like the proxy.</p>"""),

 ("Helping Someone Who Has Lost Their Password",
  "<p>A user has forgotten their password, and the instance sends no mail, so the link on the login page does nothing for them.</p>",
  """<p>On the Admin page, under <strong>Users</strong>, press <strong>Password reset link</strong>
beside them and send them the link. It works once, for a day; with it they choose a new
password and are logged out everywhere else.</p>
<p>If it is the admin who has lost the password, run beside the instance's
<code>docker-compose.yml</code>:</p>
<pre>docker compose exec legejo legejo reset-password &lt;username&gt;</pre>
<p>It prints such a link for any user.</p>""",
  """<p>The link begins with the public address when there is one; otherwise put the
instance's address in front of it. Until the link is used, the old password keeps working.
Both ways are noted in the system log (Recipe 12.5).</p>"""),

 ("Looking Up a Setting",
  "<p>You remember that there is a variable for it, but not its name.</p>",
  """<table>
<tr><th>Variable</th><th>Default</th><th>What it is</th></tr>
<tr><td><code>DATABASE_URL</code></td><td>SQLite in the data directory</td><td><code>postgres://…</code> or <code>sqlite://…</code></td></tr>
<tr><td><code>LEGEJO_DATA_DIR</code></td><td><code>/data</code></td><td>Book files, covers and the SQLite database</td></tr>
<tr><td><code>LEGEJO_ADDR</code></td><td><code>0.0.0.0:3000</code></td><td>Address to listen on</td></tr>
<tr><td><code>LEGEJO_PUBLIC_URL</code></td><td></td><td>The public <code>https://</code> address</td></tr>
<tr><td><code>LEGEJO_SECURE_COOKIES</code></td><td>on with <code>https</code></td><td><code>false</code> to serve over plain HTTP</td></tr>
<tr><td><code>LEGEJO_ADMIN_USER</code></td><td><code>admin</code></td><td>First admin, made at the start with the password below</td></tr>
<tr><td><code>LEGEJO_ADMIN_PASSWORD</code></td><td></td><td>Its password; unset, the first visitor creates the admin in the browser</td></tr>
<tr><td><code>LEGEJO_MAX_UPLOAD_MB</code></td><td><code>200</code></td><td>Largest accepted upload</td></tr>
<tr><td><code>LEGEJO_LOG_FORMAT</code></td><td><code>text</code></td><td><code>json</code> for one object per line</td></tr>
<tr><td><code>LEGEJO_IMPORT_DIR</code></td><td></td><td>Folder watched for books</td></tr>
<tr><td><code>LEGEJO_AUDIOBOOK_IMPORT_DIR</code></td><td></td><td>Folder watched for audiobooks</td></tr>
<tr><td><code>LEGEJO_MCP</code></td><td><code>false</code></td><td>The server for AI assistants</td></tr>
<tr><td><code>LEGEJO_FULLTEXT</code></td><td><code>false</code></td><td>Searching inside the books</td></tr>
<tr><td><code>LEGEJO_SHELF_SUGGESTIONS</code></td><td>on</td><td><code>off</code> hides the suggested shelves</td></tr>
<tr><td><code>LEGEJO_AI_API_KEY</code></td><td></td><td>Turns on the librarian</td></tr>
<tr><td><code>LEGEJO_SMTP_HOST</code></td><td></td><td>Mail server; mail is off when unset</td></tr>
<tr><td><code>LEGEJO_OIDC_ISSUER</code></td><td></td><td>Single sign-on; off when unset</td></tr>
<tr><td><code>LEGEJO_METRICS_TOKEN</code></td><td></td><td>Turns on <code>/metrics</code></td></tr>
</table>""",
  """<p>All of them are optional, and any of them can be read from a file by adding
<code>_FILE</code> to its name. The README in the repository has the full list.</p>"""),
 ]),
]

APPENDIX = """
<p>A cookbook should say what the kitchen is not equipped for.</p>
<dl>
<dt>Legejo does not read a folder of books in place.</dt>
<dd>It keeps its own copy of every book, so that it can repair the file and write your
corrections back into it. If your folder structure should stay the source of truth, Legejo is
not the right tool.</dd>
<dt>Legejo reads PDF files and comics plainly.</dt>
<dd>They open in the browser, but without the EPUB reader's settings or reading aloud, and a PDF
does not sync to a Kobo.</dd>
<dt>Legejo has no listening app.</dt>
<dd>Audiobooks are served as podcast feeds, for the podcast app you already use.</dd>
<dt>Legejo does not open copy-protected books.</dt>
<dd>Files with DRM are refused on upload.</dd>
<dt>Legejo does not phone home.</dt>
<dd>There is no telemetry and no paid tier. The only time anything asks the outside world a
question on its own is when you press a button that says it will.</dd>
</dl>
"""

COLOPHON = """
<p>The animals on the cover of <em>The Legejo Cookbook</em> are rock doves
(<em>Columba livia</em>), better known as pigeons. They are the birds of Legejo's own mark, and
were chosen for it because few creatures are as good at finding their way home with something
you gave them to carry.</p>
<p>Rock doves have lived beside people for several thousand years, first on cliffs and then on
the buildings people put up, which to a pigeon are cliffs with better food. They navigate by the
sun, by the earth's magnetic field and by landmarks, and a trained bird will return to its loft
from hundreds of kilometres away. For most of history that made them the fastest way to send a
message. They mate for life, share the sitting on the eggs, and can be taught to recognise
themselves in a mirror, which is more than can be said for most software.</p>
<p>The pigeons on the cover are not endangered. All the same, be kind to them.</p>
<p>The cover is set in the manner of a kind of computer book many of us learned from, with
affection and without permission; it is a pastiche, and this book has nothing to do with the
publisher of those. The name <em>Legejo</em> is Esperanto for a place where one reads.</p>
<p>The text is set in whatever typeface your reader prefers. Legejo is free software under the
GNU Affero General Public License.</p>
"""
