#!/usr/bin/env python3
"""Build The Legejo Cookbook, the manual that a new Legejo puts on its first
shelf (server/src/welcome.rs), as an EPUB 3 with an EPUB 2 table of contents
for older readers. The text is in content.py. Run it after changing the text,
and for every new minor version of Legejo: the book says which one it covers,
and a test in the server holds it to that.

The cover image is made separately from cover.html, when that changes:

    chrome --headless=new --window-size=1600,2400 --screenshot=cover.png cover.html
    sips -s format jpeg -s formatOptions 88 cover.png --out cover.jpg

Usage: python3 cookbook/build.py [out.epub]
"""
import re, sys, zipfile, html, xml.dom.minidom
from pathlib import Path
from content import PREFACE, CHAPTERS, APPENDIX, COLOPHON

HERE = Path(__file__).parent
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE.parent / "server/assets/the-legejo-cookbook.epub"
# The version of Legejo the book covers: major.minor of the server being built.
VERSION = ".".join(re.search(r'^version = "(\d+)\.(\d+)\.', (HERE.parent / "server/Cargo.toml").read_text(), re.M).groups())
TITLE = "The Legejo Cookbook"
AUTHOR = "The Pigeons of Legejo"
UUID = "urn:uuid:6c656765-6a6f-4c0b-8b00-636f6f6b626f"
MODIFIED = "2026-10-11T00:00:00Z"

CSS = """
body { font-family: Georgia, "Times New Roman", serif; line-height: 1.5; margin: 0 5%; }
h1, h2, h3, h4, th { font-family: "Helvetica Neue", Helvetica, Arial, sans-serif; }
h1 { font-size: 1.9em; line-height: 1.15; margin: 2.5em 0 1em; font-weight: 300; }
h1 .label { display: block; font-size: .5em; letter-spacing: .12em; text-transform: uppercase; color: #824575; font-weight: 700; margin-bottom: .5em; }
h2 { font-size: 1.25em; margin: 2.2em 0 .4em; padding-top: .7em; border-top: 2px solid #824575; }
h2 .num { color: #824575; margin-right: .5em; }
h3 { font-size: .82em; letter-spacing: .1em; text-transform: uppercase; margin: 1.3em 0 .2em; color: #444; }
p { margin: .5em 0; }
code, pre { font-family: "SF Mono", Menlo, Consolas, monospace; font-size: .86em; }
pre { white-space: pre-wrap; word-wrap: break-word; background: #f2f4f4; padding: .7em .9em; border-left: 3px solid #824575; margin: .8em 0; }
table { border-collapse: collapse; width: 100%; margin: 1em 0; font-size: .9em; }
th, td { text-align: left; vertical-align: top; padding: .35em .5em; border-bottom: 1px solid #ccc; }
ol, ul { padding-left: 1.4em; }
li { margin: .3em 0; }
dt { font-weight: bold; margin-top: 1em; }
dd { margin: .2em 0 0 0; }
.note { border: 1px solid #824575; padding: .7em .9em; margin: 1.2em 0; font-size: .95em; }
.titlepage { text-align: center; margin-top: 12%; }
.titlepage h1 { font-family: Georgia, serif; font-size: 2.6em; margin: .4em 0 .2em; }
.titlepage .sub { font-style: italic; font-size: 1.15em; }
.titlepage img { width: 45%; max-width: 15em; margin: 2em auto 1.5em; }
.titlepage .by { font-style: italic; margin-top: 2.5em; }
.titlepage .imprint { font-family: "Helvetica Neue", Helvetica, Arial, sans-serif; letter-spacing: .3em; font-weight: 700; margin-top: 3em; }
.about { font-size: .88em; margin-top: 20%; }
.cover { margin: 0; padding: 0; text-align: center; }
.cover img { max-width: 100%; max-height: 100%; }
nav ol { list-style: none; padding-left: 0; }
nav ol ol { padding-left: 1.3em; font-size: .93em; }
nav a { text-decoration: none; color: inherit; }
"""

def page(title, body, cls=""):
    return f"""<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en" lang="en">
<head><meta charset="utf-8"/><title>{html.escape(title)}</title><link rel="stylesheet" type="text/css" href="style.css"/></head>
<body{f' class="{cls}"' if cls else ''}>
{body}
</body>
</html>
"""

files = []   # (name, title, xhtml, in toc?)
toc = []     # (file, label, [(anchor, label)])

files.append(("cover.xhtml", "Cover", page("Cover", '<div class="cover"><img src="cover.jpg" alt="The Legejo Cookbook: two pigeons above the title"/></div>', "cover")))
files.append(("title.xhtml", TITLE, page(TITLE, f"""<div class="titlepage">
<h1>{TITLE}</h1>
<p class="sub">Recipes for a Library of Your Own</p>
<img src="pigeons.svg" alt="Two pigeons"/>
<p class="by">{AUTHOR}</p>
<p class="imprint">LEGEJO</p>
</div>""")))
files.append(("about.xhtml", "About This Book", page("About This Book", f"""<div class="about">
<p><strong>{TITLE}</strong><br/>by {AUTHOR}</p>
<p>Covers Legejo {VERSION}.</p>
<p>This book is in the public domain. Those who wrote it have given up every right to it that
the law lets them give up, under CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/).
Copy it, change it, print it, sell it; nobody needs to be asked.</p>
<p>Legejo itself is free software under the GNU Affero General Public License, version 3. Its
source is at https://github.com/jularboolean/legejo, which is also where to say that something
in this book is wrong.</p>
<p>The cover is a pastiche of a well-known series of computer books. This book is not published
by, connected with or approved by the publisher of that series.</p>
</div>""")))
files.append(("preface.xhtml", "Preface", page("Preface", f"<h1>Preface</h1>\n{PREFACE}")))
toc.append(("preface.xhtml", "Preface", []))

for n, (title, intro, recipes) in enumerate(CHAPTERS, 1):
    parts = [f'<h1><span class="label">Chapter {n}</span>{html.escape(title)}</h1>', intro]
    entries = []
    for m, (rtitle, problem, solution, discussion) in enumerate(recipes, 1):
        anchor = f"r{n}-{m}"
        parts.append(f'<section id="{anchor}">\n<h2><span class="num">{n}.{m}</span>{html.escape(rtitle)}</h2>')
        parts.append(f"<h3>Problem</h3>\n{problem}")
        parts.append(f"<h3>Solution</h3>\n{solution}")
        if discussion:
            parts.append(f"<h3>Discussion</h3>\n{discussion}")
        parts.append("</section>")
        entries.append((anchor, f"{n}.{m} {rtitle}"))
    name = f"ch{n:02d}.xhtml"
    files.append((name, title, page(title, "\n".join(parts))))
    toc.append((name, f"{n}. {title}", entries))

files.append(("appendix.xhtml", "What Legejo Does Not Do", page("Appendix", f'<h1><span class="label">Appendix</span>What Legejo Does Not Do</h1>\n{APPENDIX}')))
toc.append(("appendix.xhtml", "Appendix: What Legejo Does Not Do", []))
files.append(("colophon.xhtml", "Colophon", page("Colophon", f"<h1>Colophon</h1>\n{COLOPHON}")))
toc.append(("colophon.xhtml", "Colophon", []))

# Every page must be well-formed XML, or readers refuse the book.
for name, _, body in files:
    try:
        xml.dom.minidom.parseString(body.replace("<!DOCTYPE html>", "").encode())
    except Exception as e:
        sys.exit(f"{name}: {e}")

def nav_items():
    out = []
    for f, label, entries in toc:
        sub = ""
        if entries:
            sub = "<ol>" + "".join(f'<li><a href="{f}#{a}">{html.escape(l)}</a></li>' for a, l in entries) + "</ol>"
        out.append(f'<li><a href="{f}">{html.escape(label)}</a>{sub}</li>')
    return "\n".join(out)

nav = page("Table of Contents", f"""<nav epub:type="toc" id="toc"><h1>Table of Contents</h1>
<ol>
{nav_items()}
</ol></nav>
<nav epub:type="landmarks" hidden="hidden"><ol>
<li><a epub:type="cover" href="cover.xhtml">Cover</a></li>
<li><a epub:type="toc" href="nav.xhtml">Table of Contents</a></li>
<li><a epub:type="bodymatter" href="preface.xhtml">Start of the book</a></li>
</ol></nav>""")

def ncx():
    points, i = [], 0
    for f, label, entries in toc:
        i += 1
        inner = ""
        for a, l in entries:
            i += 1
            inner += f'<navPoint id="n{i}" playOrder="{i}"><navLabel><text>{html.escape(l)}</text></navLabel><content src="{f}#{a}"/></navPoint>'
        own = i - len(entries)
        points.append(f'<navPoint id="n{own}" playOrder="{own}"><navLabel><text>{html.escape(label)}</text></navLabel><content src="{f}"/>{inner}</navPoint>')
    return f"""<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
<head><meta name="dtb:uid" content="{UUID}"/><meta name="dtb:depth" content="2"/></head>
<docTitle><text>{TITLE}</text></docTitle>
<navMap>{''.join(points)}</navMap>
</ncx>
"""

manifest = "\n".join(f'<item id="{n.split(".")[0]}" href="{n}" media-type="application/xhtml+xml"/>' for n, _, _ in files)
spine = "\n".join(f'<itemref idref="{n.split(".")[0]}"{" linear=\"no\"" if n == "cover.xhtml" else ""}/>' for n, _, _ in files[:3]) + '\n<itemref idref="nav"/>\n' + "\n".join(f'<itemref idref="{n.split(".")[0]}"/>' for n, _, _ in files[3:])
opf = f"""<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid" xml:lang="en">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="bookid">{UUID}</dc:identifier>
<dc:title>{TITLE}</dc:title>
<dc:creator>{AUTHOR}</dc:creator>
<dc:language>en</dc:language>
<dc:publisher>Legejo</dc:publisher>
<dc:date>2026-10-11</dc:date>
<dc:subject>Legejo</dc:subject>
<dc:subject>User manuals</dc:subject>
<dc:rights>Public domain (CC0 1.0 Universal)</dc:rights>
<dc:source>https://github.com/jularboolean/legejo</dc:source>
<dc:description>The user manual for Legejo, the self-hosted library for your e-books, as a book of recipes: how to fill the library, describe and find books, share shelves, read in the browser, put Legejo on your home screen, connect a Kobo or KOReader, and run an instance of your own. Covers Legejo {VERSION}.</dc:description>
<meta property="dcterms:modified">{MODIFIED}</meta>
<meta name="cover" content="cover-image"/>
</metadata>
<manifest>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
<item id="css" href="style.css" media-type="text/css"/>
<item id="cover-image" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/>
<item id="pigeons" href="pigeons.svg" media-type="image/svg+xml"/>
{manifest}
</manifest>
<spine toc="ncx">
{spine}
</spine>
</package>
"""

FIXED = (2026, 10, 11, 0, 0, 0)   # the same bytes every time it is built
with zipfile.ZipFile(OUT, "w") as z:
    def put(name, data, method=zipfile.ZIP_DEFLATED):
        info = zipfile.ZipInfo(name, FIXED)
        info.compress_type = method
        info.external_attr = 0o644 << 16
        z.writestr(info, data)
    put("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
    put("META-INF/container.xml", '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    put("OEBPS/content.opf", opf)
    put("OEBPS/nav.xhtml", nav)
    put("OEBPS/toc.ncx", ncx())
    put("OEBPS/style.css", CSS)
    put("OEBPS/cover.jpg", (HERE / "cover.jpg").read_bytes(), zipfile.ZIP_STORED)
    put("OEBPS/pigeons.svg", (HERE / "pigeons.svg").read_bytes())
    for name, _, body in files:
        put(f"OEBPS/{name}", body)

recipes = sum(len(r) for _, _, r in CHAPTERS)
words = sum(len(b.split()) for _, _, b in files)
print(f"{OUT.name}: {len(CHAPTERS)} chapters, {recipes} recipes, about {words} words, {OUT.stat().st_size // 1024} kB")
