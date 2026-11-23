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
				<p class="vt-warn">{{ $t('manager.voice.remoteWarning') }}</p>
			</template>
			<template v-else-if="draft.mode === 'command'">
				<div class="vt-argv">
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
				</div>
				<label class="vt-row">
					<span>{{ $t('manager.voice.timeout') }}</span>
					<input
						v-model.number="draft.command.timeoutSecs"
						class="vt-timeout"
						type="number"
						min="1"
					/>
				</label>
				<p class="hint vt-note">{{ $t('manager.voice.commandNote') }}</p>
			</template>
			<div class="vt-actions">
				<button
					type="button"
					class="btn sm vt-save"
					:disabled="store.saving"
					@click="onSave"
				>
					{{ $t(store.saving ? 'manager.voice.saving' : 'manager.voice.save') }}
				</button>
				<button
					type="button"
					class="btn sm vt-test"
					:disabled="store.testing"
					@click="store.test()"
				>
					{{
						$t(store.testing ? 'manager.voice.testing' : 'manager.voice.test')
					}}
				</button>
			</div>
			<p v-if="store.saveError" class="mgr-error vt-save-error" role="alert">
				{{ store.saveError }}
			</p>
			<p v-if="store.testError" class="mgr-error vt-test-error" role="alert">
				{{ store.testError }}
			</p>
			<p class="vt-status">
				<b>{{ $t('manager.voice.status') }}:</b> {{ statusText }}
			</p>
			<p v-if="store.health?.lastError" class="mgr-error vt-last-error">
				{{ $t('manager.voice.lastError', { error: store.health.lastError }) }}
			</p>
			<button type="button" class="linklike vt-silero" @click="hint = !hint">
				{{ $t('manager.voice.silero') }}
			</button>
			<p v-if="hint" class="hint vt-hint">
				{{ $t('manager.voice.sileroHint') }}
			</p>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { externalVoice, type ExternalSettings } from '../tts/externalVoice';

	const clone = (s: ExternalSettings): ExternalSettings =>
		JSON.parse(JSON.stringify(s));

	export default defineComponent({
		name: 'ManagerVoiceTab',
		data() {
			return {
				store: externalVoice,
				draft: clone(externalVoice.settings),
				hint: false,
			};
		},
		computed: {
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
			'store.settings'(next: ExternalSettings) {
				this.draft = clone(next);
			},
		},
		mounted() {
			void this.store.load();
		},
		methods: {
			async onSave() {
				const d = clone(this.draft);
				d.command.argv = d.command.argv.filter((a) => a.length > 0);
				await this.store.save(d);
			},
		},
	});
</script>
