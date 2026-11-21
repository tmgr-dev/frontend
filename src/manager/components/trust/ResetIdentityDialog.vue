<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="trust-reset-title"
			@keydown="onKey"
		>
			<h2 id="trust-reset-title">{{ $t('manager.trust.reset.title') }}</h2>
			<p>{{ $t('manager.trust.reset.body') }}</p>
			<ul>
				<li>{{ $t('manager.trust.reset.newKeys') }}</li>
				<li>{{ $t('manager.trust.reset.rejoin') }}</li>
			</ul>
			<label class="trust-field">
				<span>{{ $t('manager.trust.reset.typeToConfirm', { phrase }) }}</span>
				<input
					type="text"
					data-autofocus
					autocomplete="off"
					spellcheck="false"
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
					@click="$emit('cancel')"
				>
					{{ $t('manager.trust.cancel') }}
				</button>
				<button
					type="button"
					class="btn danger"
					:disabled="busy || !enabled"
					@click="submit"
				>
					{{ $t('manager.trust.reset.confirm') }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { dialogKeydown, focusInitial } from './trustHelpers';

	const PHRASE = 'RESET';

	export default defineComponent({
		name: 'ManagerResetIdentityDialog',
		props: {
			modelValue: { type: String, default: '' },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['update:modelValue', 'confirm', 'cancel'],
		computed: {
			phrase(): string {
				return PHRASE;
			},
			enabled(): boolean {
				return this.modelValue === PHRASE;
			},
		},
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			submit() {
				if (this.enabled && !this.busy) this.$emit('confirm');
			},
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('cancel'),
				);
			},
		},
	});
</script>
