<script lang="ts">
	import { t } from '#lib/i18n';

	/** This edition's date as "YYYY", "YYYY-MM-DD" or "", always normalized. */
	let { value = $bindable() }: { value: string } = $props();

	// A full date shows the date picker; a bare year (the common case from
	// Libris and older books) the year field. The owner can switch.
	let mode = $state<'year' | 'date'>(/^\d{4}-\d{2}-\d{2}$/.test(value) ? 'date' : 'year');
	let year = $state(value.slice(0, 4));
	let date = $state(/^\d{4}-\d{2}-\d{2}$/.test(value) ? value : '');

	// Outside changes (Libris, Open Library) land in the right input.
	$effect(() => {
		const v = value;
		if (/^\d{4}-\d{2}-\d{2}$/.test(v)) {
			if (v !== date) {
				date = v;
				year = v.slice(0, 4);
				mode = 'date';
			}
		} else if (v.slice(0, 4) !== year) {
			year = v.slice(0, 4);
			if (mode === 'date') date = '';
			mode = 'year';
		}
	});

	function setYear(v: string) {
		year = v.replace(/\D/g, '').slice(0, 4);
		value = year.length === 4 ? year : year === '' ? '' : value;
	}
	function setDate(v: string) {
		date = v;
		value = v;
	}
	function switchTo(next: 'year' | 'date') {
		mode = next;
		if (next === 'year') value = year.length === 4 ? year : '';
		else if (date) value = date;
		else if (year.length === 4) {
			date = `${year}-01-01`;
			value = date;
		}
	}
</script>

<div class="edition">
	{#if mode === 'year'}
		<input
			inputmode="numeric"
			maxlength="4"
			placeholder="2015"
			value={year}
			oninput={(e) => setYear(e.currentTarget.value)}
			aria-label={t('edit.edition')}
		/>
	{:else}
		<input type="date" value={date} oninput={(e) => setDate(e.currentTarget.value)} aria-label={t('edit.edition')} />
	{/if}
	<div class="switch" role="radiogroup" aria-label={t('edit.edition')}>
		<button type="button" role="radio" aria-checked={mode === 'year'} onclick={() => switchTo('year')}>{t('edit.yearOnly')}</button>
		<button type="button" role="radio" aria-checked={mode === 'date'} onclick={() => switchTo('date')}>{t('edit.fullDate')}</button>
	</div>
</div>

<style>
	.edition {
		display: flex;
		gap: 0.5rem;
		align-items: center;
	}
	input {
		flex: 1;
		min-width: 0;
	}
	.switch {
		display: inline-flex;
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		flex-shrink: 0;
	}
	.switch button {
		background: none;
		border: none;
		border-radius: 0;
		padding: 0.3rem 0.55rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--muted);
	}
	.switch button[aria-checked='true'] {
		background: var(--accent);
		color: var(--bg);
	}
	.switch button:hover {
		filter: none;
	}
</style>
