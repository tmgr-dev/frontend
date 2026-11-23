<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="mgr-mention-title"
			aria-describedby="mgr-mention-body"
			@keydown="onKey"
		>
			<h2 id="mgr-mention-title">
				{{ $t(`manager.mention.confirm.${kind}Title`) }}
			</h2>
			<p id="mgr-mention-body">
				{{ $t(`manager.mention.confirm.${kind}Body`) }}
			</p>
			<div class="trust-actions">
				<button
					type="button"
					class="btn"
					data-autofocus
					@click="$emit('cancel')"
				>
					{{ $t('manager.trust.cancel') }}
				</button>
				<button type="button" class="btn danger" @click="$emit('confirm')">
					{{ $t(`manager.mention.confirm.${kind}Action`) }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { dialogKeydown, focusInitial } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerMentionConfirmDialog',
		props: {
			kind: { type: String as PropType<'send' | 'inject'>, required: true },
		},
		emits: ['confirm', 'cancel'],
		data: () => ({ opener: null as HTMLElement | null }),
		mounted() {
			this.opener = document.activeElement as HTMLElement | null;
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		beforeUnmount() {
			if (this.opener?.isConnected) this.opener.focus();
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
