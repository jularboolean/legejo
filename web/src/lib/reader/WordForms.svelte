<script lang="ts">
	// The table of a word's forms (GET /api/analysis/forms), the form the
	// reader selected marked. Shown in the reader's word box when asked for.
	import { t } from '#lib/i18n';
	import { loadForms, tagLabel, type Forms } from '#lib/library/analysis';

	let { language, word }: { language: string; word: string } = $props();

	let forms = $state<Forms | null | undefined>(undefined);
	$effect(() => {
		const [l, w] = [language, word];
		forms = undefined;
		loadForms(l, w).then((f) => {
			if (language === l && word === w) forms = f;
		});
	});
	const tag = (name: string) => tagLabel(name);
</script>

{#if forms}
	<div class="forms" aria-live="polite">
		{#if forms.participle && forms.of}
			<p class="of">{t('analysis.forms.of', { kind: tag(forms.participle), verb: forms.of })}</p>
		{/if}
		{#each forms.tables as table, n (n)}
			<table>
				{#if table.title.length}<caption>{table.title.map(tag).join(', ')}</caption>{/if}
				<tbody>
					{#each table.rows as row, i (i)}
						<tr class:this={row.this}>
							<th scope="row">{row.tags.map(tag).join(', ')}</th>
							<td lang={language}>{row.form}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/each}
	</div>
{:else if forms === null}
	<p class="of">{t('analysis.forms.none')}</p>
{/if}

<style>
	.forms {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem 1.2rem;
		margin-top: 0.4rem;
		font-size: 0.85rem;
		max-height: 14rem;
		overflow-y: auto;
	}
	caption {
		padding-top: 0.3rem;
		text-align: left;
		font-weight: 600;
	}
	.of {
		flex-basis: 100%;
		margin: 0.3rem 0 0;
		font-size: 0.85rem;
		opacity: 0.8;
	}
	table {
		border-collapse: collapse;
	}
	th {
		padding: 0.05rem 0.8rem 0.05rem 0;
		text-align: left;
		font-weight: 400;
		opacity: 0.7;
	}
	td {
		padding: 0.05rem 0;
	}
	tr.this td {
		font-weight: 700;
	}
</style>
