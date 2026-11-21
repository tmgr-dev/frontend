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
			<p v-for="kind in alertKinds" :key="kind" class="mgr-error" role="alert">
				{{ alertText(kind) }}
			</p>
			<p
				v-for="gap in gaps"
				:key="gap.from_device"
				class="mgr-error"
				role="alert"
			>
				{{
					$t('manager.trust.devices.seqGaps', {
						n: gap.missing,
						label: gap.label,
					})
				}}
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
					<template v-if="d.can_confirm_kind && d.state === 'trusted'">
						<button
							type="button"
							class="btn small"
							:aria-label="
								$t('manager.trust.devices.confirmDesktopLabel', {
									name: d.label,
								})
							"
							:disabled="busy"
							@click="$emit('confirmKind', d.id, 'desktop')"
						>
							{{ $t('manager.trust.devices.confirmDesktop') }}
						</button>
						<button
							type="button"
							class="btn small"
							:aria-label="
								$t('manager.trust.devices.confirmPhoneLabel', { name: d.label })
							"
							:disabled="busy"
							@click="$emit('confirmKind', d.id, 'mobile')"
						>
							{{ $t('manager.trust.devices.confirmPhone') }}
						</button>
					</template>
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
			<p v-if="joining" role="status">
				{{ $t('manager.trust.devices.waiting') }}
			</p>
			<p v-else-if="!canJoin">{{ $t('manager.trust.devices.joinedHint') }}</p>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button
					v-if="canJoin"
					type="button"
					class="btn"
					:disabled="busy"
					@click="$emit('enterCode')"
				>
					{{
						joining
							? $t('manager.trust.devices.anotherCode')
							: $t('manager.trust.devices.enterCode')
					}}
				</button>
				<button
					type="button"
					class="btn primary"
					:disabled="busy || joining"
					@click="$emit('addDevice')"
				>
					{{ $t('manager.trust.devices.addDevice') }}
				</button>
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
	import type { SeqGap, TrustAlert, TrustDevice } from '../../types';
	import { dialogKeydown, focusInitial, groupFp } from './trustHelpers';

	const ALERT_KEYS: Record<string, string> = {
		join_code_reused: 'joinCodeReused',
		room_conflict: 'roomConflict',
		kind_conflict: 'kindConflict',
		kind_unknown: 'kindUnknown',
		trust_unstable: 'trustUnstable',
		root_kind_unknown: 'rootKindUnknown',
	};

	export default defineComponent({
		name: 'ManagerDevicesPanel',
		props: {
			devices: { type: Array as PropType<TrustDevice[]>, required: true },
			keyMismatch: { type: Array as PropType<string[]>, default: () => [] },
			alerts: { type: Array as PropType<TrustAlert[]>, default: () => [] },
			seqGaps: { type: Array as PropType<SeqGap[]>, default: () => [] },
			canJoin: { type: Boolean, default: true },
			joining: { type: Boolean, default: false },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: [
			'revoke',
			'confirmKind',
			'reset',
			'close',
			'addDevice',
			'enterCode',
		],
		computed: {
			alertKinds(): string[] {
				return [...new Set(this.alerts.map((a) => a.kind))];
			},
			gaps(): { from_device: string; label: string; missing: number }[] {
				const missing = new Map<string, number>();
				for (const g of this.seqGaps) {
					missing.set(
						g.from_device,
						(missing.get(g.from_device) ?? 0) + Math.max(g.got - g.expected, 1),
					);
				}
				return [...missing].map(([from_device, n]) => ({
					from_device,
					label:
						this.devices.find((d) => d.id === from_device)?.label ??
						from_device,
					missing: n,
				}));
			},
		},
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			grouped: (fp: string) => groupFp(fp),
			alertText(kind: string): string {
				const key = ALERT_KEYS[kind];
				return key
					? this.$t(`manager.trust.devices.${key}`)
					: this.$t('manager.trust.devices.alertOther', { kind });
			},
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('close'),
				);
			},
		},
	});
</script>
