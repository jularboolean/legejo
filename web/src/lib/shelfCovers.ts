import cover1 from './assets/shelf-covers/cover1.jpg';
import cover2 from './assets/shelf-covers/cover2.jpg';
import cover3 from './assets/shelf-covers/cover3.jpg';
import cover4 from './assets/shelf-covers/cover4.jpg';
import cover5 from './assets/shelf-covers/cover5.jpg';
import cover6 from './assets/shelf-covers/cover6.jpg';
import cover7 from './assets/shelf-covers/cover7.jpg';
import cover8 from './assets/shelf-covers/cover8.jpg';
import cover9 from './assets/shelf-covers/cover9.jpg';

/** The covers on offer, for the picker on the shelf's edit page. */
export const shelfCovers = [cover1, cover2, cover3, cover4, cover5, cover6, cover7, cover8, cover9];
const covers = shelfCovers;

/**
 * Default cover for a shelf without an uploaded one. Keyed on the shelf id,
 * so the same shelf always gets the same picture (no reshuffle on reload)
 * and existing shelves get covers retroactively.
 */
export function defaultShelfCover(id: number): string {
	return covers[defaultShelfCoverIndex(id)];
}

export function defaultShelfCoverIndex(id: number): number {
	return Math.abs(id) % covers.length;
}
