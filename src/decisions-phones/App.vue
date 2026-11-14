<template>
	<div class="phones">
		<header class="bar" data-tauri-drag-region>
			<span data-tauri-drag-region>{{ $t('decisions.phones.title') }}</span>
		</header>
		<main class="body">
			<p v-if="s.error" class="err" role="alert">{{ s.error }}</p>

			<section v-if="s.pairing" class="card pairing" aria-live="polite">
				<h2>{{ headline }}</h2>
				<template v-if="s.pairing.state === 'waiting'">
					<img
						class="qr"
						:src="s.pairing.qr_svg_data_url"
						:alt="$t('decisions.phones.qrAlt')"
					/>
					<p class="muted" data-test="qr-hint">{{ $t('decisions.phones.qrHint') }}</p>
					<p class="muted">
						<i18n-t keypath="decisions.phones.expiresInTime" scope="global">
							<template #time>
								<strong data-test="countdown">{{ countdown }}</strong>
							</template>
						</i18n-t>
					</p>
				</template>
				<form
					v-else-if="s.pairing.state === 'confirm'"
					class="confirm"
					@submit.prevent="submit"
				>
					<label for="code">{{ $t('decisions.phones.typeCode') }}</label>
					<input
						id="code"
						v-model="code"
						inputmode="numeric"
						autocomplete="off"
						maxlength="6"
						placeholder="000000"
						@input="code = sanitizeCode(code)"
					/>
					<p class="muted">
						{{ attemptsLabel(s.pairing.attempts_left) }} ·
						<strong data-test="countdown">{{ countdown }}</strong>
					</p>
					<button type="submit" class="primary" :disabled="!valid || s.busy">
						{{ $t('decisions.phones.confirm') }}
					</button>
				</form>
				<p v-else-if="s.pairing.state === 'done'" class="muted">
					{{ $t('decisions.phones.done') }}
				</p>
				<div class="row">
					<button type="button" :disabled="s.busy" @click="c.cancelPairing()">
						{{
							active
								? $t('decisions.phones.cancel')
								: $t('decisions.phones.close')
						}}
					</button>
				</div>
			</section>

			<section class="card">
				<h2>{{ $t('decisions.phones.pairedPhones') }}</h2>
				<p v-if="!s.phones.length" class="muted">
					{{ $t('decisions.phones.none') }}
				</p>
				<ul v-else class="list">
					<li v-for="p in s.phones" :key="p.id">
						<div class="info">
							<strong>{{ p.name }}</strong>
							<span class="muted mono">{{
								shortFingerprint(p.fingerprint)
							}}</span>
							<span class="muted">
								{{
									$t('decisions.phones.pairedInfo', {
										date: formatDate(p.paired_at_ms),
										since: sinceLabel(p.last_used_ms, now),
									})
								}}
							</span>
						</div>
						<div v-if="s.revokeId === p.id" class="row" role="alertdialog">
							<span>{{
								$t('decisions.phones.revokeQuestion', { name: p.name })
							}}</span>
							<button
								type="button"
								class="danger"
								data-test="revoke-yes"
								:disabled="s.busy"
								@click="c.confirmRevoke()"
							>
								{{ $t('decisions.phones.revoke') }}
							</button>
							<button type="button" @click="c.cancelRevoke()">
								{{ $t('decisions.phones.keep') }}
							</button>
						</div>
						<button
							v-else
							type="button"
							data-test="revoke"
							:disabled="s.busy"
							@click="c.askRevoke(p.id)"
						>
							{{ $t('decisions.phones.revoke') }}
						</button>
					</li>
				</ul>
			</section>
		</main>
	</div>
</template>

<script lang="ts">
	import { currentLocale } from '@/i18n';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';
	import {
		onPhonesChanged,
		phonesCancel,
		phonesConfirm,
		phonesList,
		phonesPairing,
		phonesRevoke,
	} from '@/phones/api';
	import { isValidCode, sanitizeCode } from '@/phones/code';
	import { createPhonesController } from '@/phones/controller';
	import {
		attemptsLabel,
		countdownLabel,
		pairingHeadline,
		shortFingerprint,
		sinceLabel,
	} from '@/phones/view';

	export default defineComponent({
		name: 'PhonesWindow',
		setup() {
			const c = createPhonesController({
				list: phonesList,
				pairing: phonesPairing,
				confirm: phonesConfirm,
				cancel: phonesCancel,
				revoke: phonesRevoke,
			});
			const s = c.state;
			const code = ref('');
			const now = ref(Date.now());
			let tick: ReturnType<typeof setInterval> | undefined;
			let unlisten: (() => void) | undefined;

			const valid = computed(() => isValidCode(code.value));
			const active = computed(
				() => s.pairing?.state === 'waiting' || s.pairing?.state === 'confirm',
			);
			const headline = computed(() =>
				s.pairing ? pairingHeadline(s.pairing) : '',
			);
			const countdown = computed(() =>
				s.pairing ? countdownLabel(s.pairing.expires_ms, now.value) : '',
			);
			const formatDate = (ms: number) =>
				new Date(ms).toLocaleDateString(currentLocale(), {
					year: 'numeric',
					month: 'short',
					day: 'numeric',
				});

			const submit = async () => {
				const value = code.value;
				code.value = '';
				await c.submitCode(value);
			};

			onMounted(async () => {
				tick = setInterval(() => (now.value = Date.now()), 1000);
				unlisten = await onPhonesChanged(() => void c.refresh());
				await c.refresh();
			});
			onBeforeUnmount(() => {
				if (tick) clearInterval(tick);
				unlisten?.();
			});

			return {
				c,
				s,
				code,
				now,
				valid,
				active,
				headline,
				countdown,
				submit,
				formatDate,
				sanitizeCode,
				attemptsLabel,
				shortFingerprint,
				sinceLabel,
			};
		},
	});
</script>

<style>
	html,
	body,
	#root {
		height: 100%;
		margin: 0;
	}
	body {
		font-family: 'Exo 2', system-ui, sans-serif;
		font-size: 14px;
	}
	.phones {
		--bg: #f6f6f8;
		--fg: #18181b;
		--panel: #fff;
		--line: rgba(24, 24, 27, 0.14);
		--muted: #6b6b76;
		--accent: #4f46e5;
		--bad: #dc2626;
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg);
		color: var(--fg);
	}
	html.dark .phones {
		--bg: #0c0c0f;
		--fg: #f4f4f5;
		--panel: #16161a;
		--line: rgba(255, 255, 255, 0.14);
		--muted: #9d9da8;
	}
	.phones .bar {
		height: 40px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		border-bottom: 1px solid var(--line);
		font-weight: 600;
		-webkit-user-select: none;
		user-select: none;
	}
	.phones .body {
		flex: 1;
		overflow: auto;
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.phones .card {
		background: var(--panel);
		border: 1px solid var(--line);
		border-radius: 10px;
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.phones h2 {
		margin: 0;
		font-size: 15px;
	}
	.phones .muted {
		color: var(--muted);
		margin: 0;
	}
	.phones .mono {
		font-family: 'JetBrains Mono', monospace;
	}
	.phones .err {
		margin: 0;
		color: var(--bad);
	}
	.phones .qr {
		width: 240px;
		height: 240px;
		align-self: center;
		background: #fff;
		border-radius: 8px;
		image-rendering: pixelated;
	}
	.phones .confirm {
		display: flex;
		flex-direction: column;
		gap: 8px;
		align-items: flex-start;
	}
	.phones input {
		font: inherit;
		font-family: 'JetBrains Mono', monospace;
		font-size: 22px;
		letter-spacing: 0.3em;
		width: 10em;
		padding: 6px 10px;
		border-radius: 8px;
		border: 1px solid var(--line);
		background: var(--bg);
		color: var(--fg);
	}
	.phones button {
		font: inherit;
		padding: 6px 12px;
		border-radius: 8px;
		border: 1px solid var(--line);
		background: transparent;
		color: var(--fg);
		cursor: pointer;
	}
	.phones button:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.phones button.primary {
		background: var(--accent);
		border-color: var(--accent);
		color: #fff;
	}
	.phones button.danger {
		border-color: var(--bad);
		color: var(--bad);
	}
	.phones .row {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.phones .list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.phones .list li {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
	}
	.phones .info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
</style>
