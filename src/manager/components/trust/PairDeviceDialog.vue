<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="trust-pair-title"
			@keydown="onKey"
		>
			<template v-if="mode === 'show'">
				<h2 id="trust-pair-title">{{ $t('manager.trust.pair.showTitle') }}</h2>
				<p>{{ $t('manager.trust.pair.showBody') }}</p>
				<p v-if="expired" class="mgr-error" role="alert">
					{{ $t('manager.trust.pair.expired') }}
				</p>
				<template v-else-if="code">
					<img
						class="trust-qr"
						:src="qr"
						:alt="$t('manager.trust.pair.qrAlt')"
					/>
					<p class="trust-countdown" role="timer">
						{{ $t('manager.trust.pair.expiresIn', { time: countdown }) }}
					</p>
					<label class="trust-field">
						<span>{{ $t('manager.trust.pair.codeLabel') }}</span>
						<input
							type="text"
							readonly
							autocomplete="off"
							spellcheck="false"
							class="mono"
							:value="code"
							@focus="($event.target as HTMLInputElement).select()"
						/>
					</label>
				</template>
				<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
				<div class="trust-actions">
					<button type="button" class="btn" @click="$emit('close')">
						{{ $t('manager.trust.close') }}
					</button>
					<button
						v-if="expired"
						type="button"
						class="btn primary"
						:disabled="busy"
						@click="$emit('renew')"
					>
						{{ $t('manager.trust.pair.newCode') }}
					</button>
					<button
						v-else-if="code"
						type="button"
						class="btn primary"
						data-autofocus
						@click="copy"
					>
						{{
							copied
								? $t('manager.trust.pair.copied')
								: $t('manager.trust.pair.copy')
						}}
					</button>
				</div>
			</template>
			<template v-else>
				<h2 id="trust-pair-title">{{ $t('manager.trust.pair.enterTitle') }}</h2>
				<p>{{ $t('manager.trust.pair.enterBody') }}</p>
				<label class="trust-field">
					<span>{{ $t('manager.trust.pair.enterLabel') }}</span>
					<input
						type="text"
						data-autofocus
						autocomplete="off"
						spellcheck="false"
						class="mono"
						:value="modelValue"
						@input="
							$emit(
								'update:modelValue',
								($event.target as HTMLInputElement).value,
							)
						"
						@keydown.enter.prevent="submit"
					/>
				</label>
				<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
				<div class="trust-actions">
					<button
						type="button"
						class="btn"
						:disabled="busy"
						@click="$emit('close')"
					>
						{{ $t('manager.trust.cancel') }}
					</button>
					<button
						type="button"
						class="btn primary"
						:disabled="busy || !modelValue.trim()"
						@click="submit"
					>
						{{ $t('manager.trust.pair.join') }}
					</button>
				</div>
			</template>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { dialogKeydown, focusInitial } from './trustHelpers';

	export default defineComponent({
		name: 'ManagerPairDeviceDialog',
		props: {
			mode: { type: String as PropType<'show' | 'enter'>, required: true },
			code: { type: String, default: '' },
			qr: { type: String, default: '' },
			secondsLeft: { type: Number, default: 0 },
			expired: { type: Boolean, default: false },
			modelValue: { type: String, default: '' },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['update:modelValue', 'submit', 'renew', 'close'],
		data() {
			return { copied: false };
		},
		computed: {
			countdown(): string {
				const m = Math.floor(this.secondsLeft / 60);
				const s = String(this.secondsLeft % 60).padStart(2, '0');
				return `${m}:${s}`;
			},
		},
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			async copy() {
				try {
					await navigator.clipboard.writeText(this.code);
					this.copied = true;
				} catch {
					this.copied = false;
				}
			},
			submit() {
				if (this.modelValue.trim() && !this.busy) this.$emit('submit');
			},
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('close'),
				);
			},
		},
	});
</script>
