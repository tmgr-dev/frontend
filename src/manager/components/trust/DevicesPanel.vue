<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog wide"
			role="dialog"
			aria-modal="true"
			aria-labelledby="trust-devices-title"
			@keydown="onKey"
		>
			<h2 id="trust-devices-title">{{ $t('manager.trust.devices.title') }}</h2>
			<p>{{ $t('manager.trust.devices.hint') }}</p>
			<p v-if="keyMismatch.length > 0" class="mgr-error" role="alert">
				{{ $t('manager.trust.devices.keyMismatch') }}
			</p>
			<p
				v-if="alerts.some((a) => a.kind === 'join_code_reused')"
				class="mgr-error"
				role="alert"
			>
				{{ $t('manager.trust.devices.joinCodeReused') }}
			</p>
			<ul class="trust-devices">
				<li v-for="d in devices" :key="d.id" class="trust-device">
					<div class="t">
						<b
							>{{ d.label }}
							<em v-if="d.is_self">{{
								$t('manager.trust.devices.thisDevice')
							}}</em></b
						>
						<small
							>{{ d.kind }} ·
							<span class="mono trust-fp">{{ grouped(d.fp) }}</span></small
						>
					</div>
					<span :class="['pill', `tr-${d.state}`]">{{
						$t(`manager.trust.state.${d.state}`)
					}}</span>
					<button
						v-if="!d.is_self && d.state !== 'revoked'"
						type="button"
						class="btn small"
						:aria-label="
							$t('manager.trust.devices.revokeLabel', { name: d.label })
						"
						:disabled="busy"
						@click="$emit('revoke', d.id)"
					>
						{{ $t('manager.trust.devices.revoke') }}
					</button>
				</li>
				<li v-if="!devices.length" class="trust-empty">
					{{ $t('manager.trust.devices.empty') }}
				</li>
			</ul>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button
					type="button"
					class="btn danger"
					:disabled="busy"
					@click="$emit('reset')"
				>
					{{ $t('manager.trust.devices.reset') }}
				</button>
				<button
					type="button"
					class="btn"
					data-autofocus
					@click="$emit('close')"
				>
					{{ $t('manager.trust.close') }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { TrustAlert, TrustDevice } from '../../types';
	import { dialogKeydown, focusInitial, groupFp } from './trustHelpers';

	export default defineComponent({
		name: 'ManagerDevicesPanel',
		props: {
			devices: { type: Array as PropType<TrustDevice[]>, required: true },
			keyMismatch: { type: Array as PropType<string[]>, default: () => [] },
			alerts: { type: Array as PropType<TrustAlert[]>, default: () => [] },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['revoke', 'reset', 'close'],
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			grouped: (fp: string) => groupFp(fp),
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('close'),
				);
			},
		},
	});
</script>
