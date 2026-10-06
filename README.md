<p align="center">
  <img src="web/src/lib/assets/legejo.svg" alt="Legejo" width="150">
</p>

# Legejo

Legejo is a self-hosted library for EPUB books. Upload your books, organize
them on shelves and read them in the browser, on a Kobo, in KOReader or in any
OPDS reading app, with your reading position following you between devices.

It runs as a single container with SQLite or PostgreSQL and is built with Rust
(Axum) and SvelteKit.

![The library view](docs/screenshots/library.jpg)

## Why Legejo?

- **Fast.** Unlike most alternatives, Legejo is not a web front end on top of an
  existing e-book manager. It is built from the ground up in Rust and SvelteKit
  for performance, and it feels instant.
- **Light.** One small container and no runtime to tune. A running instance idles
  at around 30 MB of memory, so it is at home on a Raspberry Pi or the smallest VPS.
- **Easy to self-host.** `docker compose up` and you are done: SQLite is built in
  and needs no configuration. PostgreSQL is there if you want it.
- **Made for e-readers.** Kobo sync, KOReader sync and OPDS are core features, not
  plugins, and your reading position follows you between them.
- **Fediverse ready.** Connect your instance to the fediverse: shelves of free
  books can be followed from other Legejo instances and from Mastodon, over
  ActivityPub.
- **MCP server built in.** Let the AI assistant you already use search your
  library and read your books, through a read-only Model Context Protocol
  server. No AI runs inside Legejo, and it is off until you turn it on.
- **Free.** Open source under the AGPL, with no paid tier and no telemetry.
- **Actively developed.** Legejo is under active development; issues and pull
  requests are welcome.

## Why not?

Legejo does one thing: EPUB. It does not handle PDF, comics, audiobooks or
any other format. That is on purpose: the goal is a fast, smooth application
for EPUB books that runs on its own, with no other services to set up. If
your library is mostly PDFs or comics, another tool will serve you better.

## Features

**Library**
- Upload EPUB files; title, author, series, cover and other metadata are read from the file.
- Edit metadata by hand or look it up in Open Library or Libris (the Swedish national catalogue).
- The file follows the catalog: title, authors, language, series and cover are written back into the EPUB, so it is right on the e-reader and in exports too.
- A health check on every upload: a missing table of contents, language, identifier or cover declaration is repaired, and broken links are reported. Copy-protected files are refused.
- A book whose file claims copyright is marked as protected from the start.
- Series, tags, ratings, a want-to-read list and an edition date picker.
- Full-text search, filters, sorting, an authors page and duplicate detection on upload.
- Select many books at once to tag, shelve, mark as want-to-read or delete them.

**Shelves**
- Private shelves, shelves shared with the users you choose, and shelves shared with everyone on the instance.
- Users can copy books from shared shelves into their own library.

**Reading**
- A web reader with themes, font settings and saved position.
- Reading aloud in the web reader, with the device's own voices: the sentence being read is marked, and the pages turn along.
- A reading page with what you are reading, what you want to read, and statistics.
- **Kobo sync:** Legejo acts as the Kobo store for your e-reader. Books, shelves (as collections) and reading progress sync both ways, and books are converted to KEPUB on the fly.
- **KOReader sync:** a kosync-compatible progress server for KOReader on Kobo, PocketBook, Onyx Boox, Kindle and Android.
- **OPDS 1.2** catalog for reading apps such as KOReader and Moon+ Reader, with a separate app password per app.
- **AI assistants (optional):** a read-only [MCP](https://modelcontextprotocol.io) server lets an assistant you already use look up your books, shelves and reading progress, and read or search the text of a book.

![The reading page](docs/screenshots/reading.jpg)

**Federation**
- Shelves of free books (public domain or Creative Commons, with a source) can be federated over ActivityPub. Other Legejo instances, and Mastodon users, can follow them and import the books.
- Admins choose whether federation is on and which instances are allowed or blocked.
- Follows to or from an instance that is not allowed yet wait for the admin, who sees them as requests. A followed shelf whose instance stops answering is marked, and dropped after a week.

**Accounts and administration**
- Multiple users with admin roles, invitations and optional self-registration with email verification.
- Optional single sign-on through any OpenID Connect provider (Pocket ID, Authentik, Keycloak, Forgejo, ...).
- An export of the whole library (EPUB files, covers, CSV and JSON) and account deletion.
- A system log of everything that happens on the instance.
- Interface in English, Swedish, Finnish, German, French, Spanish and Esperanto.

**For self-hosters**
- SQLite with zero configuration, or PostgreSQL.
- A watched import folder for EPUB files.
- Prometheus metrics, JSON logs, throttling of password guessing, and secrets from files.

## Running Legejo with Docker

```sh
curl -O https://raw.githubusercontent.com/jularboolean/legejo/main/docker-compose.yml
docker compose up -d
docker compose logs legejo | grep -A3 Bootstrap
```

Open <http://localhost:3000> and log in as `admin` with the password printed in
the log. Books and the database are stored in the `legejo-data` volume.

For PostgreSQL instead of SQLite, use
[`docker-compose.postgres.yml`](docker-compose.postgres.yml). An existing SQLite
installation can be moved to PostgreSQL with the bundled `import-sqlite` tool:

```sh
docker compose -f docker-compose.postgres.yml run --rm legejo import-sqlite /data/legejo.db
```

Put Legejo behind a reverse proxy that terminates TLS, and set
`LEGEJO_PUBLIC_URL` to the address users reach it on. Kobo sync, links in
emails and federation depend on it.

The image is published for `linux/amd64` and `linux/arm64` as
[`slurvdjur/legejo`](https://hub.docker.com/r/slurvdjur/legejo) on Docker Hub, tagged with the version (`1.0.0`, `1.0`, `1`) and `latest`.
It has a health check built in, so Docker and deployment platforms can tell
when the server is ready.

### Coolify, Dokploy and similar platforms

Legejo runs as a single container, so it fits platforms that deploy a Docker
image or a Compose file behind their own reverse proxy:

- **Image:** `slurvdjur/legejo:1`, or one of the Compose files with the
  `ports` mapping removed.
- **Port:** route your domain to container port `3000`.
- **Storage:** a persistent volume mounted at `/data`.
- **Environment:**
  - `LEGEJO_PUBLIC_URL`: the `https://` address of the domain.
  - `LEGEJO_ADMIN_PASSWORD`: the admin password, so you do not have to find
    the generated one in the logs. It is only used when the database is empty.
  - `LEGEJO_CLIENT_IP_HEADER=X-Forwarded-For`: lets login throttling work per
    client. Traefik and Caddy, which these platforms use, replace that header
    in their default configuration; leave the variable unset if yours is
    configured to trust it from clients.

With SQLite, the database lives in the same volume as the books. For
PostgreSQL, add `DATABASE_URL`.

### Configuration

Everything is configured with environment variables, and all of them are
optional. Any variable can instead be read from a file by appending `_FILE` to
its name (for example `LEGEJO_ADMIN_PASSWORD_FILE=/run/secrets/admin`), which
works with Docker and Kubernetes secrets.

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | SQLite in the data directory | `postgres://user:password@host/db` or `sqlite://path` |
| `LEGEJO_DATA_DIR` | `/data` | Book files, covers and the SQLite database |
| `LEGEJO_ADDR` | `0.0.0.0:3000` | Address to listen on |
| `LEGEJO_PUBLIC_URL` | | The public `https://` address of the instance |
| `LEGEJO_SECURE_COOKIES` | on with an `https` public URL | Mark session cookies `Secure`. Set to `false` to serve over plain HTTP |
| `LEGEJO_ADMIN_USER` | `admin` | Initial admin account, created when the database is empty |
| `LEGEJO_ADMIN_PASSWORD` | generated | Its password; printed once in the log when generated |
| `LEGEJO_MAX_UPLOAD_MB` | `200` | Largest accepted upload |
| `LEGEJO_LOG_FORMAT` | `text` | `json` for one JSON object per line. The level is set with `RUST_LOG` |
| `LEGEJO_MCP` | `false` | `true` turns on the MCP endpoint for AI assistants |

**AI assistants (MCP).** With `LEGEJO_MCP=true`, Legejo serves a read-only Model
Context Protocol endpoint at `/api/mcp` (Streamable HTTP). It is off by default.
It gives an assistant nine tools:
search the library, book details, shelves, a reading overview, a book's table
of contents, reading a section, searching inside a book, and the shelves
other users share with you and the books on them. Each user creates
an app password under Account and gives it to the assistant as a bearer token;
the assistant then sees that user's library and nothing else. For example, in
Claude Code:

```sh
claude mcp add --transport http legejo https://books.example.org/api/mcp \
  --header "Authorization: Bearer <app password>"
```

Clients that reserve the `Authorization` header for OAuth can send the app
password in an `X-Api-Key` header instead. OAuth sign-in itself is not supported.
The text of a book only leaves the server when the assistant asks for it.

**Mail** (self-registration, invitations, password reset):

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_SMTP_HOST` | | SMTP server; mail is off when unset |
| `LEGEJO_SMTP_PORT` | `587` | |
| `LEGEJO_SMTP_USER` | | Leave out for servers without authentication |
| `LEGEJO_SMTP_PASSWORD` | | |
| `LEGEJO_SMTP_TLS` | `starttls` | `starttls`, `tls` or `none` |
| `LEGEJO_MAIL_FROM` | | Sender, e.g. `Legejo <noreply@example.org>` |

**Single sign-on (OpenID Connect).** Off unless the issuer is set. Password
login keeps working beside it. Register `<LEGEJO_PUBLIC_URL>/api/auth/oidc/callback`
as the redirect URI at the provider; ID tokens must be signed with RS256.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_OIDC_ISSUER` | | e.g. `https://id.example.org` |
| `LEGEJO_OIDC_CLIENT_ID` | | |
| `LEGEJO_OIDC_CLIENT_SECRET` | | |
| `LEGEJO_OIDC_NAME` | `SSO` | Button label: "Log in with ..." |
| `LEGEJO_OIDC_AUTO_CREATE` | `false` | Create accounts for unknown users |
| `LEGEJO_OIDC_TRUST_EMAIL` | `false` | Link accounts by email even when the provider does not mark it verified |
| `LEGEJO_OIDC_ADMIN_GROUP` | | Members of this group become admins |
| `LEGEJO_OIDC_SCOPES` | `openid profile email` | `groups` is added when an admin group is set |

A user signing in for the first time is matched to an existing account with the
same verified email address, or can link their account from the account page.

**Login throttling.** Failed logins on the web and over OPDS are counted. When
the limit is reached within the window, further attempts are answered with
HTTP 429 until the window has passed.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_LOGIN_MAX_FAILURES` | `10` | Failures per username (and client IP, when known). `0` turns throttling off |
| `LEGEJO_LOGIN_MAX_FAILURES_IP` | `50` | Failures per client IP across usernames, and per username across IPs |
| `LEGEJO_LOGIN_WINDOW_MINUTES` | `15` | |
| `LEGEJO_CLIENT_IP_HEADER` | | Header with the client IP set by your reverse proxy, e.g. `X-Real-IP`. Only set it if the proxy always overwrites it |

**Import folder.** EPUB files placed in the folder (subfolders included) are
added to a user's library once they have stopped changing. Afterwards they are
moved to `imported/`, `duplicates/` or `failed/` inside the folder.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_IMPORT_DIR` | | The folder; must be writable |
| `LEGEJO_IMPORT_USER` | oldest admin | Who gets the books |
| `LEGEJO_IMPORT_INTERVAL` | `60` | Seconds between scans |
| `LEGEJO_IMPORT_DELETE` | `false` | Delete imported files instead of moving them |

**Metrics.** Prometheus metrics at `/metrics`: users, books, storage, shelves,
federation queue, exports, failed logins and activity per event type.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_METRICS_TOKEN` | | Enables `/metrics`, requiring `Authorization: Bearer <token>` |
| `LEGEJO_METRICS` | `false` | Enables `/metrics` without a token; for private networks only |

## Building from source

```sh
cd web && npm ci && npm run build
cd ../server && LEGEJO_WEB_DIR=../web/build cargo run --release
```

Requires Rust and Node.js 24. [kepubify](https://github.com/pgaskin/kepubify)
in `PATH` enables KEPUB conversion for Kobo devices.

## Built with

Legejo stands on other people's work, above all:

- [Axum](https://github.com/tokio-rs/axum) and [SQLx](https://github.com/launchbadge/sqlx) for the server.
- [SvelteKit](https://svelte.dev) for the web interface.
- [epub.js](https://github.com/futurepress/epub.js) for rendering books in the web reader.
- [kepubify](https://github.com/pgaskin/kepubify) for converting books to KEPUB for Kobo devices.
- [Lucide](https://lucide.dev) for the icons, and the [Inter](https://rsms.me/inter/) and [Fraunces](https://fonts.google.com/specimen/Fraunces) typefaces.

## License

Legejo is free software under the [GNU Affero General Public License v3.0](LICENSE).
