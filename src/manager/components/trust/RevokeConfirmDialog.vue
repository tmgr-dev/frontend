<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="trust-revoke-title"
			@keydown="onKey"
		>
			<h2 id="trust-revoke-title">
				{{
					revokerLabel
						? $t('manager.trust.revoke.titleIncoming')
						: $t('manager.trust.revoke.titleOutgoing')
				}}
			</h2>
			<p v-if="revokerLabel">
				{{
					$t('manager.trust.revoke.bodyIncoming', {
						revoker: revokerLabel,
						target: targetLabel,
					})
				}}
			</p>
			<p v-else>
				{{ $t('manager.trust.revoke.bodyOutgoing', { target: targetLabel }) }}
			</p>
			<dl class="trust-facts">
				<template v-if="revokerLabel">
					<dt>{{ $t('manager.trust.revoke.revoker') }}</dt>
					<dd>
						{{ revokerLabel }}
						<span class="mono trust-fp">{{ revokerGrouped }}</span>
					</dd>
				</template>
				<dt>{{ $t('manager.trust.revoke.target') }}</dt>
				<dd>
					{{ targetLabel }}
					<span class="mono trust-fp">{{ targetGrouped }}</span>
				</dd>
			</dl>
			<p>{{ $t('manager.trust.revoke.effect') }}</p>
			<p class="trust-warn">{{ $t('manager.trust.revoke.cascade') }}</p>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button
					type="button"
					class="btn"
					data-autofocus
					:disabled="busy"
					@click="$emit('reject')"
				>
					{{
						revokerLabel
							? $t('manager.trust.revoke.reject')
							: $t('manager.trust.cancel')
					}}
				</button>
				<button
					type="button"
					class="btn danger"
					:disabled="busy"
					@click="$emit('approve')"
				>
					{{ $t('manager.trust.revoke.approve') }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { dialogKeydown, focusInitial, groupFp } from './trustHelpers';

	export default defineComponent({
		name: 'ManagerRevokeConfirmDialog',
		props: {
			revokerLabel: { type: String, default: '' },
			revokerFp: { type: String, default: '' },
			targetLabel: { type: String, required: true },
			targetFp: { type: String, required: true },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['approve', 'reject'],
		computed: {
			revokerGrouped(): string {
				return groupFp(this.revokerFp);
			},
			targetGrouped(): string {
				return groupFp(this.targetFp);
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
