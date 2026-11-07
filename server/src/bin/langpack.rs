//! Finish a language pack made by tools/langpack/extract.py: turn its
//! forms.tsv (form, lemma numbers) into forms.fst, the compact map Legejo
//! looks forms up in, with the forms of more than one lemma listed apart
//! in ambiguous.tsv.
//!
//!     cargo run --release --bin langpack <pack dir>

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

/// A form's value in the fst: a lemma's number, or with this bit set the
/// number of a line in ambiguous.tsv.
const AMBIGUOUS: u64 = 1 << 40;

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: langpack <pack dir>"))?);
    let forms = BufReader::new(File::open(dir.join("forms.tsv"))?);
    let mut builder = fst::MapBuilder::new(BufWriter::new(File::create(dir.join("forms.fst"))?))?;
    let mut ambiguous = BufWriter::new(File::create(dir.join("ambiguous.tsv"))?);
    let (mut count, mut many) = (0u64, 0u64);
    for line in forms.lines() {
        let line = line?;
        let Some((form, lemmas)) = line.split_once('\t') else { continue };
        let value = if lemmas.contains(',') {
            writeln!(ambiguous, "{lemmas}")?;
            many += 1;
            AMBIGUOUS | (many - 1)
        } else {
            lemmas.parse()?
        };
        builder.insert(form, value)?;
        count += 1;
    }
    builder.finish()?;
    ambiguous.flush()?;
    std::fs::remove_file(dir.join("forms.tsv"))?;
    println!("{count} forms, {many} of more than one lemma");
    Ok(())
}
