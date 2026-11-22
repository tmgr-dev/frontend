<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="mgr-control-title"
			aria-describedby="mgr-control-body"
			:aria-busy="busy"
			@keydown="onKey"
		>
			<h2 id="mgr-control-title">{{ $t(`manager.control.${mode}Title`) }}</h2>
			<p id="mgr-control-body">{{ $t(`manager.control.${mode}Body`) }}</p>
			<p v-if="mode === 'delete'">
				<b>{{ $t('manager.control.deleteTarget', { title }) }}</b>
			</p>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<p v-if="busy && waitHint" class="hint" role="status">
				{{ $t('manager.control.busyHint') }}
			</p>
			<div class="trust-actions">
				<button type="button" class="btn" data-autofocus @click="cancel">
					{{ $t('manager.trust.cancel') }}
				</button>
				<button
					type="button"
					class="btn danger"
					:disabled="busy"
					@click="$emit('confirm')"
				>
					{{
						busy
							? $t('manager.control.working')
							: $t(
									mode === 'delete'
										? 'manager.control.confirmDelete'
										: 'manager.control.confirmForce',
								)
					}}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { dialogKeydown, focusInitial } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerControlDialog',
		props: {
			mode: { type: String as PropType<'force' | 'delete'>, required: true },
			title: { type: String, default: '' },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['confirm', 'cancel'],
		data: () => ({
			waitHint: false,
			opener: null as HTMLElement | null,
		}),
		mounted() {
			this.opener = document.activeElement as HTMLElement | null;
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		beforeUnmount() {
			if (this.opener?.isConnected) this.opener.focus();
		},
		methods: {
			cancel() {
				if (this.busy) this.waitHint = true;
				else this.$emit('cancel');
			},
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, this.cancel);
			},
		},
	});
</script>
