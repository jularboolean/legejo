# The Legejo Cookbook

The user manual, as an EPUB that a new Legejo puts in its first library
(`server/src/welcome.rs`). It is in the public domain (CC0 1.0).

- `content.py`: the text, as chapters of recipes.
- `build.py`: makes `server/assets/the-legejo-cookbook.epub` from it. Needs
  only Python 3.
- `cover.html`, `pigeons.svg`: the cover, and `cover.jpg`, the picture made
  from it (the commands are at the top of `build.py`).

Run `python3 cookbook/build.py` after changing the text, and for every new
minor version of Legejo: the book says which version it covers, and a test in
the server fails until it has been built again. That is the moment to read
through what the version changed and bring the recipes up to date.

The cover is a pastiche of a well-known series of computer books. The book is
not connected with their publisher, and says so.
