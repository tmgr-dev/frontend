<template>
	<div class="e2e">
		<div class="hero">
			<Icon name="lock" />
			<div>
				<b>{{ $t('manager.header.encrypted') }}</b>
				<small>{{ $t('manager.panel.samePairing') }}</small>
			</div>
		</div>
		<div class="blk">
			<h4>{{ $t('manager.panel.serverSees') }}</h4>
			<p>{{ $t('manager.panel.serverSeesBody') }}</p>
		</div>
		<div class="blk warn">
			<h4>{{ $t('manager.panel.providerSees') }}</h4>
			<p>{{ $t('manager.panel.providerSeesBody') }}</p>
		</div>
		<div class="blk">
			<h4>{{ $t('manager.panel.pairedDevices') }}</h4>
			<div v-for="d in devices" :key="d.id" class="dev">
				<span class="di"><Icon name="lock" /></span>
				<div>
					<b>{{ d.label }}</b
					><em v-if="d.is_self">{{
						$t('manager.trust.devices.thisDevice')
					}}</em>
					<small>{{ d.kind }}</small>
				</div>
				<span :class="['pill', `tr-${d.state}`]">{{
					$t(`manager.trust.state.${d.state}`)
				}}</span>
			</div>
			<p v-if="!devices.length" class="hint">
				{{ $t('manager.trust.devices.empty') }}
			</p>
			<div v-if="selfFp" class="fp">
				<span>{{ $t('manager.panel.keyFingerprint') }}</span
				><b class="mono">{{ grouped }}</b>
			</div>
			<button type="button" class="btn sm" @click="$emit('devices')">
				{{ $t('manager.panel.manageDevices') }}
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { TrustDevice } from '../types';
	import Icon from './Icon.vue';
	import { groupFp } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerEncryptionTab',
		components: { Icon },
		props: {
			selfFp: { type: String, default: '' },
			devices: { type: Array as PropType<TrustDevice[]>, default: () => [] },
		},
		emits: ['devices'],
		computed: {
			grouped(): string {
				return groupFp(this.selfFp);
			},
		},
	});
</script>
