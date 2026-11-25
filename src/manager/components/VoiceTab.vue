<template>
	<div class="e2e vt">
		<div class="blk">
			<h4>{{ $t('manager.voice.title') }}</h4>
			<p>{{ $t('manager.voice.intro') }}</p>
			<label class="vt-row">
				<span>{{ $t('manager.voice.mode') }}</span>
				<select v-model="draft.mode" class="vt-mode">
					<option value="off">{{ $t('manager.voice.modeOff') }}</option>
					<option value="http">{{ $t('manager.voice.modeHttp') }}</option>
					<option value="command">{{ $t('manager.voice.modeCommand') }}</option>
				</select>
			</label>
			<template v-if="draft.mode === 'http'">
				<label class="vt-row">
					<span>{{ $t('manager.voice.baseUrl') }}</span>
					<input v-model="draft.http.baseUrl" class="vt-url" type="text" />
				</label>
				<label class="vt-row">
					<span>{{ $t('manager.voice.model') }}</span>
					<input v-model="draft.http.model" class="vt-model" type="text" />
				</label>
				<label class="vt-row">
					<span>{{ $t('manager.voice.voice') }}</span>
					<input v-model="draft.http.voice" class="vt-voice" type="text" />
				</label>
				<label class="vt-row">
					<input
						v-model="draft.http.allowRemote"
						class="vt-remote"
						type="checkbox"
					/>
					<span>{{ $t('manager.voice.allowRemote') }}</span>
				</label>
				<p v-if="showRemoteWarning" class="vt-warn">
					{{ $t('manager.voice.remoteWarning') }}
				</p>
			</template>
			<template v-else-if="draft.mode === 'command'">
				<fieldset class="vt-argv">
					<legend>{{ $t('manager.voice.arguments') }}</legend>
					<span>{{ $t('manager.voice.argv') }}</span>
					<div v-for="(_, i) in draft.command.argv" :key="i" class="vt-row">
						<input
							v-model="draft.command.argv[i]"
							class="vt-arg"
							type="text"
							:aria-label="$t('manager.voice.argument', { n: i + 1 })"
						/>
						<button
							type="button"
							class="btn sm vt-remove"
							:aria-label="$t('manager.voice.removeArgument', { n: i + 1 })"
							@click="draft.command.argv.splice(i, 1)"
						>
							&times;
						</button>
					</div>
					<button
						type="button"
						class="btn sm vt-add"
						@click="draft.command.argv.push('')"
					>
						{{ $t('manager.voice.addArgument') }}
					</button>
				</fieldset>
				<p class="hint vt-empty-note">
					{{ $t('manager.voice.emptyArgumentsNote') }}
				</p>
				<label class="vt-row">
					<span>{{ $t('manager.voice.timeout') }}</span>
					<input
						v-model.number="draft.command.timeoutSecs"
						class="vt-timeout"
						type="number"
						min="1"
						max="120"
						step="1"
					/>
				</label>
				<p v-if="timeoutInvalid" class="mgr-error vt-timeout-error">
					{{ $t('manager.voice.timeoutInvalid') }}
				</p>
				<p class="hint vt-note">{{ $t('manager.voice.commandNote') }}</p>
			</template>
			<div class="vt-actions">
				<button
					type="button"
					class="btn sm vt-save"
					:disabled="store.saving || timeoutInvalid"
					@click="onSave"
				>
					{{ $t(store.saving ? 'manager.voice.saving' : 'manager.voice.save') }}
				</button>
				<button
					type="button"
					class="btn sm vt-test"
					:disabled="store.testing || dirty"
					@click="onTest"
				>
					{{
						$t(store.testing ? 'manager.voice.testing' : 'manager.voice.test')
					}}
				</button>
			</div>
			<label class="vt-row">
				<span>{{ $t('manager.voice.testPhrase') }}</span>
				<input v-model="phrase" class="vt-phrase" type="text" />
			</label>
			<p class="hint vt-test-note">{{ $t('manager.voice.testNote') }}</p>
			<p v-if="store.saveError" class="mgr-error vt-save-error" role="alert">
				{{ store.saveError }}
			</p>
			<p v-if="store.testError" class="mgr-error vt-test-error" role="alert">
				{{ store.testError }}
			</p>
			<p class="vt-status" aria-live="polite">
				<b>{{ $t('manager.voice.status') }}:</b> {{ statusText }}
			</p>
			<p
				v-if="store.health?.lastError"
				class="mgr-error vt-last-error"
				aria-live="polite"
			>
				{{ $t('manager.voice.lastError', { error: store.health.lastError }) }}
			</p>
			<button type="button" class="linklike vt-silero" @click="hint = !hint">
				{{ $t('manager.voice.silero') }}
			</button>
			<p v-if="hint" class="hint vt-hint">
				{{ $t('manager.voice.sileroHint') }}
			</p>
		</div>
		<div v-if="packs.length" class="blk vt-packs">
			<h4>{{ $t('manager.voice.onDeviceTitle') }}</h4>
			<div
				v-for="p in packs"
				:key="p.lang"
				:class="['vt-row', `vt-pack-${p.lang}`]"
			>
				<span>{{ languageName(p.lang) }}</span>
				<template v-if="p.installing">
					<span>{{
						$t('manager.messages.voiceDownloading', {
							percent: Math.round(p.progress * 100),
						})
					}}</span>
					<button type="button" class="btn sm" @click="p.cancel()">
						{{ $t('manager.messages.cancelVoiceDownload') }}
					</button>
				</template>
				<template v-else-if="p.installed">
					<span>{{ $t('manager.voice.packInstalled') }}</span>
					<button type="button" class="btn sm" @click="onRemovePack(p)">
						{{
							$t('manager.messages.removeVoice', {
								language: languageName(p.lang),
							})
						}}
					</button>
				</template>
				<template v-else>
					<span v-if="p.error" class="mgr-error">{{
						$t('manager.messages.voiceDownloadFailed', {
							language: languageName(p.lang),
						})
					}}</span>
					<button type="button" class="btn sm" @click="p.install()">
						{{
							$t('manager.messages.downloadVoice', {
								language: languageName(p.lang),
								size: Math.round(p.sizeBytes / 1024 / 1024),
							})
						}}
					</button>
				</template>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { externalVoice, type ExternalSettings } from '../tts/externalVoice';
	import { speaker } from '../tts/speaker';
	import { voicePacks, type PackLang } from '../tts/voicePack';

	const DEFAULT_TIMEOUT = 30;
	const LOOPBACK = /^(localhost|127(\.\d{1,3}){3}|\[::1\]|::1)$/i;

	const clone = (s: ExternalSettings): ExternalSettings =>
		JSON.parse(JSON.stringify(s));

	const same = (a: ExternalSettings, b: ExternalSettings): boolean =>
		JSON.stringify(a) === JSON.stringify(b);

	const isLoopback = (baseUrl: string): boolean => {
		try {
			return LOOPBACK.test(new URL(baseUrl).hostname);
		} catch {
			return false;
		}
	};

	const timeoutValue = (v: unknown): number | null | 'empty' => {
		if (v === '' || v === null || v === undefined) return 'empty';
		return Number.isInteger(v) && (v as number) >= 1 && (v as number) <= 120
			? (v as number)
			: null;
	};

	export default defineComponent({
		name: 'ManagerVoiceTab',
		data() {
			return {
				store: externalVoice,
				draft: clone(externalVoice.settings),
				hint: false,
				phrase: this.$t('manager.voice.testPhraseDefault') as string,
			};
		},
		computed: {
			packs() {
				return [voicePacks.en, voicePacks.ru].filter((p) => p.supported);
			},
			dirty(): boolean {
				return !same(this.draft, this.store.settings);
			},
			showRemoteWarning(): boolean {
				const http = this.draft.http;
				if (http.allowRemote) return true;
				return http.baseUrl.trim() !== '' && !isLoopback(http.baseUrl.trim());
			},
			timeoutInvalid(): boolean {
				return (
					this.draft.mode === 'command' &&
					timeoutValue(this.draft.command.timeoutSecs) === null
				);
			},
			statusText(): string {
				const h = this.store.health;
				if (!h || h.mode === 'off') return this.$t('manager.voice.statusOff');
				if (!h.configured) return this.$t('manager.voice.statusNotConfigured');
				return h.healthy
					? this.$t('manager.voice.statusReady')
					: this.$t('manager.voice.statusFailing');
			},
		},
		watch: {
			'store.settings'(next: ExternalSettings, prev: ExternalSettings) {
				if (same(this.draft, prev)) this.draft = clone(next);
			},
		},
		mounted() {
			void this.store.load();
			for (const p of [voicePacks.en, voicePacks.ru]) {
				if (p.statusRequested) continue;
				p.statusRequested = true;
				void p.refresh();
			}
		},
		methods: {
			languageName(lang: PackLang): string {
				return this.$t(`manager.messages.languageName.${lang}`);
			},
			onRemovePack(p: (typeof voicePacks)[PackLang]) {
				speaker.stop();
				void p.remove();
			},
			async onSave() {
				const d = clone(this.draft);
				d.command.argv = d.command.argv
					.map((a) => a.trim())
					.filter((a) => a.length > 0);
				const timeout = timeoutValue(d.command.timeoutSecs);
				d.command.timeoutSecs =
					typeof timeout === 'number' ? timeout : DEFAULT_TIMEOUT;
				if (await this.store.save(d)) this.draft = clone(this.store.settings);
			},
			onTest() {
				const text = this.phrase.trim();
				void this.store.test(text || undefined);
			},
		},
	});
</script>
