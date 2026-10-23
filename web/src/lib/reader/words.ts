// Words in a run of text, for picking a passage by whole words where the
// browser does not select in the book itself.

/**
 * The word at `offset` in a text, as offsets into it: the word the offset
 * falls in, or the one it ends. Null between words and on punctuation.
 */
export function wordSpan(
  text: string,
  offset: number,
  language = "",
): [number, number] | null {
  const Segmenter = (Intl as unknown as { Segmenter?: typeof Intl.Segmenter })
    .Segmenter;
  let before: [number, number] | null = null;
  if (Segmenter) {
    let segmenter: Intl.Segmenter;
    try {
      segmenter = new Segmenter(language || undefined, { granularity: "word" });
    } catch {
      segmenter = new Segmenter(undefined, { granularity: "word" });
    }
    for (const part of segmenter.segment(text)) {
      if (part.index > offset) break;
      if (!part.isWordLike) continue;
      const end = part.index + part.segment.length;
      if (offset < end) return [part.index, end];
      if (offset === end) before = [part.index, end];
    }
    return before;
  }
  // Without a segmenter: letters and digits, joined by an apostrophe or a hyphen.
  const word = /[\p{L}\p{N}\p{M}]+(?:['’-][\p{L}\p{N}\p{M}]+)*/gu;
  for (let m = word.exec(text); m && m.index <= offset; m = word.exec(text)) {
    const end = m.index + m[0].length;
    if (offset < end) return [m.index, end];
    if (offset === end) before = [m.index, end];
  }
  return before;
}
