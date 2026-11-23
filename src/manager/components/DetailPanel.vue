<template>
	<aside class="det">
		<div class="tabs" role="tablist" data-tauri-drag-region>
			<button
				v-for="t in TABS"
				:key="t"
				type="button"
				role="tab"
				:aria-selected="t === tab"
				:class="{ on: t === tab }"
				@click="$emit('tab', t)"
			>
				{{ $t(`manager.panel.tabs.${t}`) }}
			</button>
		</div>
		<div class="dbody" role="tabpanel">
			<SessionTab v-if="tab === 'session'" :info="info" :messages="messages" />
			<HierarchyTab
				v-else-if="tab === 'hierarchy'"
				:rooms="rooms"
				:machines="machines"
				:active-key="activeKey"
				@select="$emit('select', $event)"
			/>
			<AuditTab
				v-else-if="tab === 'audit'"
				:entries="audit"
				:error="auditError"
			/>
			<EncryptionTab
				v-else-if="tab === 'encryption'"
				:self-fp="trust?.self_fp ?? ''"
				:devices="trust?.devices ?? []"
				@devices="$emit('devices')"
			/>
			<VoiceTab v-else />
		</div>
	</aside>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type {
		AuditEntry,
		ChatMessage,
		ManagerMachine,
		RoomInfo,
		TrustState,
	} from '../types';
	import AuditTab from './AuditTab.vue';
	import EncryptionTab from './EncryptionTab.vue';
	import HierarchyTab from './HierarchyTab.vue';
	import SessionTab from './SessionTab.vue';
	import VoiceTab from './VoiceTab.vue';

	const TABS = ['session', 'hierarchy', 'audit', 'encryption', 'voice'];

	export default defineComponent({
		name: 'ManagerDetailPanel',
		components: { AuditTab, EncryptionTab, HierarchyTab, SessionTab, VoiceTab },
		props: {
			tab: { type: String, required: true },
			rooms: { type: Array as PropType<RoomInfo[]>, required: true },
			machines: {
				type: Array as PropType<ManagerMachine[]>,
				default: () => [],
			},
			info: { type: Object as PropType<RoomInfo>, default: undefined },
			activeKey: { type: String, default: null },
			messages: { type: Array as PropType<ChatMessage[]>, default: () => [] },
			audit: { type: Array as PropType<AuditEntry[]>, default: () => [] },
			auditError: { type: String, default: '' },
			trust: { type: Object as PropType<TrustState | null>, default: null },
		},
		emits: ['tab', 'select', 'devices'],
		data: () => ({ TABS }),
	});
</script>
