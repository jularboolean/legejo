<p align="center">
  <img src="web/src/lib/assets/legejo.svg" alt="Legejo" width="150">
</p>

<p align="center">
  <a href="https://legejo.se">Website</a> ·
  <a href="https://hub.docker.com/r/slurvdjur/legejo">Docker Hub</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="docs/configuration.md">Configuration</a> ·
  <a href="server/assets/the-legejo-cookbook.epub">The Legejo Cookbook</a>
</p>

<p align="center">
  <a href="https://github.com/jularboolean/legejo/releases"><img src="https://img.shields.io/github/v/release/jularboolean/legejo?label=version" alt="Latest version"></a>
  <a href="https://hub.docker.com/r/slurvdjur/legejo"><img src="https://img.shields.io/docker/pulls/slurvdjur/legejo" alt="Docker pulls"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/jularboolean/legejo" alt="License"></a>
</p>

# Legejo

Legejo is a self-hosted library for your e-books. Upload them, put them on
shelves, and read them in the browser, on a Kobo, in KOReader or in any OPDS
app, with your place following you between devices.

It is built around EPUB, and also keeps PDF files, comics (CBZ) and
audiobooks. It runs as one small container, written in Rust and SvelteKit,
with SQLite built in.

<p>
  <img src="docs/screenshots/library.jpg" alt="The library view, light" width="49%">
  <img src="docs/screenshots/library-dark.jpg" alt="The library view, dark" width="49%">
</p>

## Features

- **Library:** metadata read from the file or looked up in Open Library and
  Libris, and written back into the EPUB. Every EPUB is checked and repaired
  on upload. Series, tags, ratings, full-text search and duplicate detection.
- **Reading:** a web reader with themes, highlights, notes and reading aloud.
  It also reads PDF files and comics, and works offline when installed on a
  phone or tablet.
- **E-readers:** Kobo sync (books, collections, progress and highlights),
  a KOReader sync server, OPDS 1.2 and Send to Kindle.
- **Shelves to share:** with users on your instance, with someone on another
  Legejo or a reading app by a private link, and, for free books, with the
  fediverse over ActivityPub.
- **Audiobooks (optional):** each one becomes a private podcast feed for the
  podcast app you already use.
- **Other libraries (optional):** browse OPDS catalogs such as Project
  Gutenberg and fetch a book into your library with one click.
- **AI (optional, off by default):** a read-only MCP server for your own
  assistant, and a librarian that suggests books from your shelves using a
  model the operator chooses.
- **Accounts:** several users, invitations, OpenID Connect sign-in, a full
  export, and an interface in English, Swedish, Finnish, German, French,
  Spanish and Esperanto.

*The Legejo Cookbook*, the user manual, comes with every new instance and
describes it all in detail.

### What it is not

- Not a comic server: CBZ only, no CBR or webtoon mode.
- Not an audiobook player: listening happens in your podcast app.
- Not a reader of your folders: Legejo keeps its own copy of each book, so
  it can repair it and write your corrections into it.

## Running Legejo

The image is [`slurvdjur/legejo`](https://hub.docker.com/r/slurvdjur/legejo)
for `linux/amd64` and `linux/arm64`.

```sh
curl -O https://raw.githubusercontent.com/jularboolean/legejo/main/docker-compose.yml
docker compose up -d
```

Open <http://localhost:3000> and create the admin account. The first visitor
of a new instance makes it, so on a server reachable from the internet, do it
straight away or set `LEGEJO_ADMIN_PASSWORD` to have it made at the start.

Behind a reverse proxy that terminates TLS, set the instance's public address
on the Admin page or as `LEGEJO_PUBLIC_URL`: Kobo sync, links in mail and
federation need it. Mail and the librarian are set up on the Admin page too.

[Configuration](docs/configuration.md) covers PostgreSQL, book files in S3 or
R2, deployment platforms such as Coolify and Dokploy, and every environment
variable.

## Building from source

```sh
cd web && npm ci && npm run build
cd ../server && LEGEJO_WEB_DIR=../web/build cargo run --release
```

Requires Rust and Node.js 24. [kepubify](https://github.com/pgaskin/kepubify)
in `PATH` enables KEPUB conversion for Kobo devices.

## Built with

[Axum](https://github.com/tokio-rs/axum), [SQLx](https://github.com/launchbadge/sqlx),
[SvelteKit](https://svelte.dev), [epub.js](https://github.com/futurepress/epub.js),
[kepubify](https://github.com/pgaskin/kepubify), [Lucide](https://lucide.dev),
[Inter](https://rsms.me/inter/) and [Fraunces](https://fonts.google.com/specimen/Fraunces).

## License

Free software under the [GNU Affero General Public License v3.0](LICENSE).
