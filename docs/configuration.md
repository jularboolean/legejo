# Configuring Legejo

Everything here is optional: Legejo runs with no configuration at all, and
mail, the public address, the librarian and search inside the books can also
be set up on the Admin page. See the [README](../README.md) for getting started.

Settings made on the Admin page are tried with a button before they are
saved. Set as environment variables instead, they are shown there and cannot
be changed in the browser: the environment always wins. Passwords and keys
saved on the Admin page are kept in the database, and so in its backups.

## PostgreSQL

Use [`docker-compose.postgres.yml`](../docker-compose.postgres.yml) instead of
the default Compose file. An existing SQLite installation can be moved to
PostgreSQL with the bundled `import-sqlite` tool:

```sh
docker compose -f docker-compose.postgres.yml run --rm legejo import-sqlite /data/legejo.db
```

## Coolify, Dokploy and similar platforms

Legejo runs as a single container, so it fits platforms that deploy a Docker
image or a Compose file behind their own reverse proxy:

- **Image:** `slurvdjur/legejo:1`, or one of the Compose files with the
  `ports` mapping removed.
- **Port:** route your domain to container port `3000`.
- **Storage:** a persistent volume mounted at `/data`.
- **Environment:**
  - `LEGEJO_PUBLIC_URL`: the `https://` address of the domain.
  - `LEGEJO_ADMIN_PASSWORD`: optional, the admin password, so the account
    exists before anyone opens the site. Without it, the first visitor creates
    the admin in the browser. It is only used when the database is empty.
  - `LEGEJO_CLIENT_IP_HEADER=X-Forwarded-For`: lets login throttling work per
    client. Legejo takes the last address in it, the one the platform's proxy
    added, so an address a client writes there itself is not believed. Behind
    a second proxy such as Cloudflare, name the header that one sets
    (`CF-Connecting-IP`) instead.

With SQLite, the database lives in the same volume as the books. For
PostgreSQL, add `DATABASE_URL`.

## The public list of instances

**Show my instance publicly**, on the Admin page, puts the instance on the list
at <https://instances.legejo.se>. It is off until you turn it on, and needs an
`https` public address. The instance then sends the list its address, and answers
`GET /api/directory` with its version, whether federation is on and whether
anyone can sign up: that is all the list knows. Turned off, that address answers 404 and the
instance leaves the list within a day.

### Testing from outside

**Test from outside**, under Public address on the Admin page, has
instances.legejo.se try the address from the internet: useful because the
ordinary test runs on the server itself, and can pass where the outside cannot
get in. It runs only when an admin presses it, and sends the address. The address
is not kept. An instance that answers is counted once a month by the random id
it gives at `/api/instance`, and those counts are dropped after a year.

## Book files in S3 or R2

Legejo can keep the book files, covers, audiobooks, avatars and exports in an
S3 bucket (Cloudflare R2, AWS, MinIO, Backblaze, Hetzner and others) instead
of a volume. With PostgreSQL for the database, the container then keeps
nothing it cannot lose, and runs on platforms without a persistent disk.

```sh
LEGEJO_STORAGE=s3://my-bucket/legejo
AWS_ACCESS_KEY_ID=…
AWS_SECRET_ACCESS_KEY=…
AWS_REGION=auto                                          # R2; your region elsewhere
AWS_ENDPOINT_URL=https://<account>.r2.cloudflarestorage.com  # leave out for AWS
DATABASE_URL=postgres://…
```

Every file is uploaded when it is written and removed when it is deleted;
`/data` becomes a cache that files are fetched to when they are first read,
and where the KEPUB copies and thumbnails made from them stay. Audiobooks are
played straight from the bucket, a range at a time. The cache is not trimmed:
give it room for the files people read, or let a restart start it afresh.

To move an existing library, stop Legejo and run, with the same variables
set:

```sh
docker compose run --rm legejo legejo storage migrate
```

It copies every file of `/data` that belongs in the bucket, skips those
already there, and can be run again. SQLite still works with a bucket, but
the database is then the one thing left in `/data`: keep that on a volume.

## Environment variables

Everything is configured with environment variables, and all of them are
optional. Any variable can instead be read from a file by appending `_FILE` to
its name (for example `LEGEJO_ADMIN_PASSWORD_FILE=/run/secrets/admin`), which
works with Docker and Kubernetes secrets.

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | SQLite in the data directory | `postgres://user:password@host/db` or `sqlite://path` |
| `LEGEJO_DATA_DIR` | `/data` | Book files, covers and the SQLite database; with `LEGEJO_STORAGE`, a cache of the bucket |
| `LEGEJO_STORAGE` | | `s3://bucket/prefix`: keep book files, covers, audiobooks, avatars and exports in an S3 bucket, with the `AWS_*` variables above |
| `LEGEJO_ADDR` | `0.0.0.0:3000` | Address to listen on |
| `LEGEJO_PUBLIC_URL` | | The public `https://` address of the instance |
| `LEGEJO_SECURE_COOKIES` | on with an `https` public URL | Mark session cookies `Secure`. Set to `false` to serve over plain HTTP |
| `LEGEJO_ADMIN_USER` | `admin` | Initial admin account, made at the start when the database is empty and `LEGEJO_ADMIN_PASSWORD` is set |
| `LEGEJO_ADMIN_PASSWORD` | | Its password. Unset, the first visitor creates the admin in the browser |
| `LEGEJO_MAX_UPLOAD_MB` | `200` | Largest accepted upload |
| `LEGEJO_LOG_FORMAT` | `text` | `json` for one JSON object per line. The level is set with `RUST_LOG` |
| `LEGEJO_KOBO_TRACE` | `false` | `true` logs what is said with a Kobo, headers and bodies, for finding out why a sync went wrong. Tokens, keys, addresses and what the device reports of its use are left out; titles, places and highlights are not, so turn it off when done |
| `LEGEJO_MCP` | `false` | `true` turns on the MCP endpoint for AI assistants |
| `LEGEJO_FULLTEXT` | | Searching inside the books, also a switch on the Admin page. `true` or `false` here wins over the switch. While on, the text of the EPUBs is kept in the database, once per text; switched off, the text is dropped |
| `LEGEJO_AI_API_KEY` | | Turns on the librarian. The key of the account that pays |
| `LEGEJO_AI_BASE_URL` | `https://api.openai.com/v1` | The provider's chat API, for the librarian |
| `LEGEJO_AI_MODEL` | `gpt-5.4-mini` | The model the librarian is |

**The librarian.** Once a model is set up (on the Admin page, or with
`LEGEJO_AI_API_KEY`), users can turn on the
librarian from their account page. Each question sends the catalogue of that
user's books to the model, in parts, so the cost of a question grows with the
size of the library: a few hundred books take some twenty thousand tokens.
Libraries over 500 books are gone through in two steps, a quick pass by title
and author and a close reading of what that turns up, which costs about a
quarter as much per book. A user can ask 30 questions an hour.

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

**Mail** (self-registration, invitations, password reset), also on the Admin page:

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

An account created at sign-on has no password. Its owner changes or deletes it
without one for 15 minutes after signing in, and is asked to sign in again after
that. A password chosen on the account page is then required like any other.

**Login throttling.** Failed logins on the web and over OPDS are counted. When
the limit is reached within the window, further attempts are answered with
HTTP 429 until the window has passed.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_LOGIN_MAX_FAILURES` | `10` | Failures per username (and client IP, when known). `0` turns throttling off |
| `LEGEJO_LOGIN_MAX_FAILURES_IP` | `50` | Failures per client IP across usernames, and per username across IPs |
| `LEGEJO_LOGIN_WINDOW_MINUTES` | `15` | |
| `LEGEJO_CLIENT_IP_HEADER` | | Header with the client IP set by your reverse proxy, e.g. `X-Real-IP`. For a list such as `X-Forwarded-For`, the last address is taken: the one the proxy in front of Legejo added |

**Import folder.** EPUB, PDF and CBZ files placed in the folder (subfolders included) are
added to a user's library once they have stopped changing. Afterwards they are
moved to `imported/`, `duplicates/` or `failed/` inside the folder.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_IMPORT_DIR` | | The folder; must be writable |
| `LEGEJO_IMPORT_USER` | oldest admin | Who gets the books |
| `LEGEJO_IMPORT_INTERVAL` | `60` | Seconds between scans |
| `LEGEJO_IMPORT_DELETE` | `false` | Delete imported files instead of moving them |

**Import folder for audiobooks.** Each entry at the top of the folder becomes one
audiobook: a folder with its mp3, m4a or m4b files (subfolders such as `CD 1` included,
in the order of their names), or a single audio file. An image beside the files becomes
the cover. An entry is taken once nothing in it has changed for a minute, and is then
moved to `imported/`, `duplicates/` or `failed/`. The audio is never held in memory, and
when the folder is on the same disk as `/data` nothing is copied, so audiobooks of
several gigabytes are fine. Audiobooks must be turned on at the admin page.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_AUDIOBOOK_IMPORT_DIR` | | The folder; must be writable |
| `LEGEJO_AUDIOBOOK_IMPORT_USER` | oldest admin | Who gets the audiobooks |
| `LEGEJO_AUDIOBOOK_IMPORT_INTERVAL` | `60` | Seconds between scans |
| `LEGEJO_AUDIOBOOK_IMPORT_DELETE` | `false` | Delete imported entries instead of moving them |

**Metrics.** Prometheus metrics at `/metrics`: users, books, storage, shelves,
federation queue, exports, failed logins and activity per event type.

| Variable | Default | Description |
|---|---|---|
| `LEGEJO_METRICS_TOKEN` | | Enables `/metrics`, requiring `Authorization: Bearer <token>` |
| `LEGEJO_METRICS` | `false` | Enables `/metrics` without a token; for private networks only |
