<template>
	<div class="hier">
		<template v-for="node in nodes" :key="node.machine">
			<div :class="node.main ? 'hnode' : 'hb hsub'">
				<button
					v-if="node.manager"
					type="button"
					:class="[
						'hn',
						node.main ? 'top' : 'mid',
						{ on: keyOf(node.manager) === activeKey },
					]"
					@click="$emit('select', keyOf(node.manager))"
				>
					<Avatar manager />
					<div>
						<b>{{ node.manager.title }}</b>
						<small>{{ roleLine(node) }}</small>
					</div>
				</button>
				<div v-if="node.manager && !node.main" class="scope">
					<Icon name="shield" /><span>{{
						$t('manager.hierarchy.localScope', { machine: node.label })
					}}</span>
				</div>
				<div v-if="node.sessions.length" :class="node.main ? 'hb' : 'hbs'">
					<div class="hl">
						{{
							node.main && node.manager
								? $t('manager.hierarchy.controlsOwn')
								: node.label
						}}
					</div>
					<div class="hss">
						<button
							v-for="info in node.sessions"
							:key="keyOf(info)"
							type="button"
							:class="[
								'hs',
								`st-${info.state ?? 'idle'}`,
								{ on: keyOf(info) === activeKey },
							]"
							@click="$emit('select', keyOf(info))"
						>
							<Avatar :agent="info.agent" size="xs" /><b>{{
								info.agent ?? info.title
							}}</b
							><i v-if="info.task">{{ info.task }}</i
							><span class="dt"></span>
						</button>
					</div>
				</div>
			</div>
		</template>
		<p v-if="!nodes.length" class="hint">
			{{ $t('manager.panel.noSessions') }}
		</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { buildHierarchy, type HierarchyNode } from '../hierarchy';
	import { roomKey, type ManagerMachine, type RoomInfo } from '../types';
	import Avatar from './Avatar.vue';
	import Icon from './Icon.vue';
	import { agentName, modelFamily } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerHierarchyTab',
		components: { Avatar, Icon },
		props: {
			rooms: { type: Array as PropType<RoomInfo[]>, required: true },
			machines: {
				type: Array as PropType<ManagerMachine[]>,
				default: () => [],
			},
			activeKey: { type: String, default: null },
		},
		emits: ['select'],
		computed: {
			nodes(): HierarchyNode[] {
				return buildHierarchy(this.rooms, this.machines);
			},
		},
		methods: {
			keyOf: (info: RoomInfo) => roomKey(info.room),
			roleLine(node: HierarchyNode): string {
				const m = node.manager as RoomInfo;
				const parts = [
					this.$t(
						node.main
							? 'manager.hierarchy.mainRole'
							: 'manager.hierarchy.subRole',
					),
					node.label,
					...(m.agent ? [agentName(m.agent)] : []),
					...(m.model ? [modelFamily(m.agent, m.model)] : []),
					this.$t(
						node.main
							? 'manager.hierarchy.mainDuty'
							: 'manager.hierarchy.subDuty',
					),
				];
				return parts.join(' · ');
			},
		},
	});
</script>
