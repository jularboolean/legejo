/** Admin → Languages: the language packs of the instance (server/src/langadmin.rs). */

export type LanguagePack = {
  code: string;
  name: string;
  version: string;
  source: string;
  license: string;
  attribution: string;
  lemmas: number;
  forms: number;
  /** The pack has extras for the analysis of books. */
  extras?: boolean;
  /** Books on the instance in the language, of every user. */
  books: number;
};

/** A pack the catalogue offers (LEGEJO_LANGUAGES_URL, by default legejo.se). */
export type AvailablePack = {
  code: string;
  /** The language's own name for itself. */
  name: string;
  english: string;
  version: string;
  lemmas: number;
  /** The size of the download. */
  bytes: number;
  license: string;
  attribution: string;
  /** The version installed, if the language has a pack. */
  installed_version: string | null;
  /** The catalogue has a newer version than the one installed. */
  update: boolean;
  /** The catalogue has extras for the pack. */
  extras?: boolean;
  books: number;
};

export type LanguagePacks = {
  packs: LanguagePack[];
  /** Built in through ESPDIC: always there. */
  esperanto: { books: number };
  /** The page about language packs on legejo.se. */
  catalogue: string;
  /** Where the server fetches the catalogue; null when it is off. */
  catalogue_url: string | null;
  /** Why the catalogue could not be fetched. */
  catalogue_error: string | null;
  available: AvailablePack[];
  /** Languages with books here and a pack in the catalogue, most books first. */
  suggested: string[];
};

/** The largest zip the server takes. */
export const MAX_PACK_ZIP_BYTES = 64 * 1024 * 1024;

/** A failed request: the status and the server's message, when it gave one. */
export class PackError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

async function failure(res: Response): Promise<PackError> {
  const body = await res.json().catch(() => null);
  return new PackError(
    res.status,
    typeof body?.error === "string" ? body.error : "",
  );
}

export async function listPacks(
  fetcher: typeof fetch = fetch,
): Promise<LanguagePacks> {
  const res = await fetcher("/api/admin/languages");
  if (!res.ok) throw await failure(res);
  return res.json();
}

export async function importPack(file: File): Promise<LanguagePack> {
  const form = new FormData();
  form.append("file", file);
  const res = await fetch("/api/admin/languages", {
    method: "POST",
    body: form,
  });
  if (!res.ok) throw await failure(res);
  return res.json();
}

/** Download the catalogue's pack for a language and install it (or update it). */
export async function installPack(code: string): Promise<LanguagePack> {
  const res = await fetch("/api/admin/languages/install", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code }),
  });
  if (!res.ok) throw await failure(res);
  return res.json();
}

export async function removePack(code: string): Promise<void> {
  const res = await fetch(`/api/admin/languages/${encodeURIComponent(code)}`, {
    method: "DELETE",
  });
  if (!res.ok) throw await failure(res);
}
