<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="trust-confirm-kind-title"
			@keydown="onKey"
		>
			<h2 id="trust-confirm-kind-title">
				{{ $t('manager.trust.confirmKind.title') }}
			</h2>
			<p>{{ $t('manager.trust.confirmKind.body', { kind: kindName }) }}</p>
			<dl class="trust-facts">
				<dt>{{ $t('manager.trust.device') }}</dt>
				<dd>{{ label }}</dd>
				<dt>{{ $t('manager.trust.kind') }}</dt>
				<dd>{{ kindName }}</dd>
				<dt>{{ $t('manager.trust.fingerprint') }}</dt>
				<dd class="mono trust-fp">{{ grouped }}</dd>
			</dl>
			<p class="trust-warn">{{ $t('manager.trust.confirmKind.compare') }}</p>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button
					type="button"
					class="btn"
					data-autofocus
					:disabled="busy"
					@click="$emit('cancel')"
				>
					{{ $t('manager.trust.cancel') }}
				</button>
				<button
					type="button"
					class="btn primary"
					:disabled="busy"
					@click="$emit('approve')"
				>
					{{ $t('manager.trust.confirmKind.approve') }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { dialogKeydown, focusInitial, groupFp } from './trustHelpers';

	export default defineComponent({
		name: 'ManagerConfirmKindDialog',
		props: {
			label: { type: String, required: true },
			fp: { type: String, required: true },
			kind: {
				type: String as PropType<'desktop' | 'mobile'>,
				required: true,
			},
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['approve', 'cancel'],
		computed: {
			grouped(): string {
				return groupFp(this.fp);
			},
			kindName(): string {
				return this.$t(
					this.kind === 'mobile'
						? 'manager.trust.confirmKind.kindMobile'
						: 'manager.trust.confirmKind.kindDesktop',
				);
			},
		},
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('cancel'),
				);
			},
		},
	});
</script>
