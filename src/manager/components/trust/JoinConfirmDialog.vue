<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="trust-join-title"
			@keydown="onKey"
		>
			<h2 id="trust-join-title">{{ $t('manager.trust.join.title') }}</h2>
			<p>{{ $t('manager.trust.join.body') }}</p>
			<dl class="trust-facts">
				<dt>{{ $t('manager.trust.device') }}</dt>
				<dd>{{ label }}</dd>
				<dt>{{ $t('manager.trust.kind') }}</dt>
				<dd>{{ kind || $t('manager.trust.unknownKind') }}</dd>
				<dt>{{ $t('manager.trust.fingerprint') }}</dt>
				<dd class="mono trust-fp">{{ grouped }}</dd>
			</dl>
			<p class="trust-warn">{{ $t('manager.trust.join.compare') }}</p>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button
					type="button"
					class="btn"
					data-autofocus
					:disabled="busy"
					@click="$emit('reject')"
				>
					{{ $t('manager.trust.join.reject') }}
				</button>
				<button
					type="button"
					class="btn primary"
					:disabled="busy"
					@click="$emit('approve')"
				>
					{{ $t('manager.trust.join.approve') }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { dialogKeydown, focusInitial, groupFp } from './trustHelpers';

	export default defineComponent({
		name: 'ManagerJoinConfirmDialog',
		props: {
			label: { type: String, required: true },
			kind: { type: String, default: '' },
			fp: { type: String, required: true },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['approve', 'reject'],
		computed: {
			grouped(): string {
				return groupFp(this.fp);
			},
		},
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('reject'),
				);
			},
		},
	});
</script>
