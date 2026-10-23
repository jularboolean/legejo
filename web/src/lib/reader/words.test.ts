import { describe, expect, it } from "vitest";
import { wordSpan } from "./words";

const word = (text: string, offset: number, language = "sv") => {
  const span = wordSpan(text, offset, language);
  return span ? text.slice(...span) : null;
};

describe("wordSpan", () => {
  it("finds the word an offset falls in", () => {
    expect(word("Det var en gång", 0)).toBe("Det");
    expect(word("Det var en gång", 5)).toBe("var");
    expect(word("Det var en gång", 13)).toBe("gång");
  });

  it("takes the word an offset ends", () => {
    expect(word("Det var en gång", 3)).toBe("Det");
    expect(word("Det var en gång", 15)).toBe("gång");
  });

  it("finds nothing between words or on punctuation", () => {
    expect(word("Det  var", 4)).toBeNull();
    expect(word("— Ja!", 0)).toBeNull();
  });

  it("leaves the punctuation around a word out", () => {
    expect(word("”Hej,” sa hon.", 2)).toBe("Hej");
    expect(word("sa hon.", 5)).toBe("hon");
  });

  it("keeps letters with diacritics in the word", () => {
    expect(word("ĉiutage ŝi legas", 2, "eo")).toBe("ĉiutage");
  });
});
