<script lang="ts" module>
	export type ConnectionsData = {
		public_url: { source: Source; value: string | null; detected: string };
		mail: {
			source: Source;
			host: string | null;
			port: number | null;
			user: string | null;
			password_set: boolean;
			tls: string | null;
			from: string | null;
		};
		librarian: { source: Source; base_url: string | null; model: string | null; key_set: boolean };
	};
	export type Source = 'env' | 'admin' | 'none';
</script>

<script lang="ts">
	// The public address, mail and the librarian's model, set up here unless
	// the environment sets them (then shown, and locked). Each has a test that
	// tries what the form says before it is saved.
	import { invalidateAll } from '$app/navigation';
	import { t } from '#lib/i18n';

	type Outcome = { ok: boolean; text: string } | null;

	let {
		connections,
		usage
	}: {
		connections: ConnectionsData;
		/** The librarian's use over the last 30 days, when it is set up. */
		usage: { questions: number; prompt_tokens: number; completion_tokens: number } | null;
	} = $props();

	// svelte-ignore state_referenced_locally
	let c = $state(connections);

	// ---- Public address ----------------------------------------------------------
	// svelte-ignore state_referenced_locally
	let url = $state(c.public_url.value ?? '');
	let urlOutcome = $state<Outcome>(null);
	let urlBusy = $state(false);

	// ---- Mail --------------------------------------------------------------------
	// svelte-ignore state_referenced_locally
	let mail = $state({
		host: c.mail.host ?? '',
		port: c.mail.port ?? 587,
		user: c.mail.user ?? '',
		password: '',
		tls: c.mail.tls === 'tls' ? 'tls' : 'starttls',
		from: c.mail.from ?? ''
	});
	/** Empty: the admin's own address, when the account has one. */
	let testTo = $state('');
	let mailOutcome = $state<Outcome>(null);
	let mailBusy = $state(false);

	// ---- Librarian ---------------------------------------------------------------
	// svelte-ignore state_referenced_locally
	let ai = $state({ base_url: c.librarian.base_url ?? '', model: c.librarian.model ?? '', key: '' });
	let aiOutcome = $state<Outcome>(null);
	let aiBusy = $state(false);

	const failed = (error: string) => ({ ok: false, text: t('conn.failed', { error }) });

	/** Send to the server; the answer's JSON, or an outcome that says what went wrong. */
	async function call(path: string, method: string, body: unknown): Promise<{ status: number; body: Record<string, unknown> } | Outcome> {
		try {
			const res = await fetch(path, { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
			return { status: res.status, body: await res.json().catch(() => ({})) };
		} catch {
			return { ok: false, text: t('common.network') };
		}
	}

	/** A refused form: which field, in words. */
	function refused(field: unknown): Outcome {
		const known: Record<string, string> = {
			url: t('conn.urlInvalid'),
			scheme: t('conn.urlScheme'),
			from: t('conn.fromInvalid'),
			to: t('conn.toMissing'),
			host: t('conn.hostMissing'),
			key: t('conn.keyMissing'),
			base_url: t('conn.baseUrlInvalid')
		};
		return { ok: false, text: known[String(field)] ?? t('edit.saveFailed') };
	}

	async function testUrl() {
		urlBusy = true;
		const r = await call('/api/admin/connections/public-url/test', 'POST', { url });
		urlBusy = false;
		if (!r || 'ok' in r) return void (urlOutcome = r);
		if (r.status !== 200) return void (urlOutcome = refused(r.body.error));
		urlOutcome = r.body.ok
			? { ok: true, text: r.body.https ? t('conn.urlOk') : t('conn.urlOkHttp') }
			: failed(String(r.body.error));
	}

	async function saveUrl() {
		urlBusy = true;
		const r = await call('/api/admin/connections/public-url', 'PUT', { url });
		urlBusy = false;
		if (!r || 'ok' in r) return void (urlOutcome = r);
		if (r.status !== 200) return void (urlOutcome = refused(r.body.error));
		c = r.body as ConnectionsData;
		url = c.public_url.value ?? '';
		urlOutcome = { ok: true, text: t('conn.saved') };
		// The federation tab shows whether the address lets it be switched on.
		await invalidateAll();
	}

	const mailBody = () => ({ ...mail, password: mail.password === '' ? undefined : mail.password });

	async function testMail() {
		mailBusy = true;
		const r = await call('/api/admin/connections/mail/test', 'POST', { ...mailBody(), to: testTo });
		mailBusy = false;
		if (!r || 'ok' in r) return void (mailOutcome = r);
		if (r.status !== 200) return void (mailOutcome = refused(r.body.error));
		mailOutcome = r.body.ok ? { ok: true, text: t('conn.mailSent', { to: String(r.body.to) }) } : failed(String(r.body.error));
	}

	async function saveMail(off = false) {
		mailBusy = true;
		const r = await call('/api/admin/connections/mail', 'PUT', off ? { host: '' } : mailBody());
		mailBusy = false;
		if (!r || 'ok' in r) return void (mailOutcome = r);
		if (r.status !== 200) return void (mailOutcome = refused(r.body.error));
		c = r.body as ConnectionsData;
		mail.password = '';
		if (off) mail = { host: '', port: 587, user: '', password: '', tls: 'starttls', from: '' };
		mailOutcome = { ok: true, text: off ? t('conn.turnedOff') : t('conn.saved') };
	}

	const aiBody = () => ({ ...ai, key: ai.key === '' ? undefined : ai.key });

	async function testAi() {
		aiBusy = true;
		const r = await call('/api/admin/connections/librarian/test', 'POST', aiBody());
		aiBusy = false;
		if (!r || 'ok' in r) return void (aiOutcome = r);
		if (r.status !== 200) return void (aiOutcome = refused(r.body.error));
		aiOutcome = r.body.ok ? { ok: true, text: t('conn.librarianOk', { model: String(r.body.model) }) } : failed(String(r.body.error));
	}

	async function saveAi(off = false) {
		aiBusy = true;
		const r = await call('/api/admin/connections/librarian', 'PUT', off ? { key: '' } : aiBody());
		aiBusy = false;
		if (!r || 'ok' in r) return void (aiOutcome = r);
		if (r.status !== 200) return void (aiOutcome = refused(r.body.error));
		c = r.body as ConnectionsData;
		ai.key = '';
		aiOutcome = { ok: true, text: off ? t('conn.turnedOff') : t('conn.saved') };
	}
</script>

<section>
	<h2>{t('conn.publicUrl')}</h2>
	<p class="hint">{t('conn.publicUrlHint')}</p>
	{#if c.public_url.source === 'env'}
		<p class="value"><code>{c.public_url.value}</code></p>
		<p class="hint">{t('conn.fromEnv', { name: 'LEGEJO_PUBLIC_URL' })}</p>
	{:else}
		<div class="row">
			<input type="url" bind:value={url} placeholder="https://books.example.org" aria-label={t('conn.publicUrl')} />
		</div>
		{#if !url && c.public_url.detected}
			<button type="button" class="suggest" onclick={() => (url = c.public_url.detected)}>
				{t('conn.useDetected', { url: c.public_url.detected })}
			</button>
		{/if}
		<div class="actions">
			<button type="button" class="ghost" onclick={testUrl} disabled={urlBusy || !url.trim()}>{t('conn.test')}</button>
			<button type="button" onclick={saveUrl} disabled={urlBusy}>{t('conn.save')}</button>
		</div>
	{/if}
	{#if urlOutcome}<p class={urlOutcome.ok ? 'ok' : 'error'}>{urlOutcome.text}</p>{/if}
</section>

<section>
	<h2>{t('admin.mail')}</h2>
	<p class="hint">{t('conn.mailHint')}</p>
	{#if c.mail.source === 'env'}
		<p class="value"><code>{c.mail.host}:{c.mail.port}</code> · {c.mail.from}</p>
		<p class="hint">{t('conn.fromEnv', { name: 'LEGEJO_SMTP_HOST' })}</p>
	{:else}
		<div class="grid">
			<label class="wide">
				{t('conn.host')}
				<input bind:value={mail.host} placeholder="smtp.example.org" autocomplete="off" />
			</label>
			<label>
				{t('conn.port')}
				<input type="number" min="1" max="65535" bind:value={mail.port} />
			</label>
			<label>
				{t('conn.security')}
				<select bind:value={mail.tls}>
					<option value="starttls">{t('conn.starttls')}</option>
					<option value="tls">{t('conn.tls')}</option>
				</select>
			</label>
			<label>
				{t('conn.user')}
				<input bind:value={mail.user} autocomplete="off" />
			</label>
			<label>
				{t('conn.password')}
				<input
					type="password"
					bind:value={mail.password}
					autocomplete="new-password"
					placeholder={c.mail.password_set ? t('conn.kept') : ''}
				/>
			</label>
			<label class="wide">
				{t('conn.from')}
				<input bind:value={mail.from} placeholder="Legejo <books@example.org>" autocomplete="off" />
			</label>
		</div>
	{/if}
	<label class="to">
		{t('conn.testTo')}
		<input type="email" bind:value={testTo} placeholder={t('conn.testToOwn')} />
	</label>
	<div class="actions">
		<button type="button" class="ghost" onclick={testMail} disabled={mailBusy}>{t('conn.testMail')}</button>
		{#if c.mail.source !== 'env'}
			<button type="button" onclick={() => saveMail()} disabled={mailBusy}>{t('conn.save')}</button>
			{#if c.mail.source === 'admin'}
				<button type="button" class="ghost" onclick={() => saveMail(true)} disabled={mailBusy}>{t('conn.turnOff')}</button>
			{/if}
		{/if}
	</div>
	{#if mailOutcome}<p class={mailOutcome.ok ? 'ok' : 'error'}>{mailOutcome.text}</p>{/if}
</section>

<section>
	<h2>{t('librarian.heading')}</h2>
	<p class="hint">{t('conn.librarianHint')}</p>
	{#if c.librarian.source === 'env'}
		<p class="value"><code>{c.librarian.model}</code> · {c.librarian.base_url}</p>
		<p class="hint">{t('conn.fromEnv', { name: 'LEGEJO_AI_API_KEY' })}</p>
	{:else}
		<div class="grid">
			<label class="wide">
				{t('conn.baseUrl')}
				<input type="url" bind:value={ai.base_url} placeholder="https://api.openai.com/v1" autocomplete="off" />
			</label>
			<label>
				{t('conn.model')}
				<input bind:value={ai.model} placeholder="gpt-5.4-mini" autocomplete="off" />
			</label>
			<label>
				{t('conn.key')}
				<input type="password" bind:value={ai.key} autocomplete="new-password" placeholder={c.librarian.key_set ? t('conn.kept') : ''} />
			</label>
		</div>
	{/if}
	<div class="actions">
		<button type="button" class="ghost" onclick={testAi} disabled={aiBusy}>{t('conn.test')}</button>
		{#if c.librarian.source !== 'env'}
			<button type="button" onclick={() => saveAi()} disabled={aiBusy}>{t('conn.save')}</button>
			{#if c.librarian.source === 'admin'}
				<button type="button" class="ghost" onclick={() => saveAi(true)} disabled={aiBusy}>{t('conn.turnOff')}</button>
			{/if}
		{/if}
	</div>
	{#if aiOutcome}<p class={aiOutcome.ok ? 'ok' : 'error'}>{aiOutcome.text}</p>{/if}
	{#if usage && c.librarian.source !== 'none'}
		<p class="hint">
			{t('admin.librarianUse', { questions: usage.questions, input: usage.prompt_tokens, output: usage.completion_tokens })}
		</p>
	{/if}
</section>

<style>
	section {
		max-width: 34rem;
		margin-bottom: 2.5rem;
	}
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.75rem;
		padding-bottom: 0.4rem;
		border-bottom: 1px solid var(--border);
	}
	.hint {
		margin: 0.4rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.value {
		margin: 0.75rem 0 0;
		font-size: 0.9rem;
		overflow-wrap: anywhere;
	}
	.row {
		margin-top: 0.75rem;
		display: flex;
	}
	.row input {
		flex: 1;
		min-width: 0;
	}
	.grid {
		margin-top: 0.75rem;
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.6rem 0.75rem;
	}
	.grid label,
	.to {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		color: var(--muted);
		min-width: 0;
	}
	.grid .wide {
		grid-column: 1 / -1;
	}
	.to {
		margin-top: 0.75rem;
	}
	@media (max-width: 480px) {
		.grid {
			grid-template-columns: 1fr;
		}
	}
	.suggest {
		margin-top: 0.4rem;
		font-size: 0.8rem;
		background: none;
		border: 1px dashed var(--border);
		color: var(--muted);
	}
	.actions {
		margin-top: 0.75rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.ok {
		color: var(--accent);
		font-size: 0.85rem;
		margin: 0.5rem 0 0;
	}
	.error {
		font-size: 0.85rem;
		margin: 0.5rem 0 0;
		overflow-wrap: anywhere;
	}
</style>
