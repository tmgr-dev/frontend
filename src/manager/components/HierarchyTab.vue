<template>
	<div class="hier">
		<template v-for="info in managers" :key="keyOf(info)">
			<button
				type="button"
				:class="['hn', 'top', { on: keyOf(info) === activeKey }]"
				@click="$emit('select', keyOf(info))"
			>
				<Avatar manager />
				<div>
					<b>{{ info.title }}</b>
					<small>{{ info.room.machine }}</small>
				</div>
			</button>
		</template>
		<div v-for="group in groups" :key="group.machine" class="hb">
			<div class="hl">{{ group.machine }}</div>
			<div class="hss">
				<button
					v-for="info in group.sessions"
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
					><i>{{ info.title }}</i
					><span class="dt"></span>
				</button>
			</div>
		</div>
		<p v-if="!groups.length" class="hint">
			{{ $t('manager.panel.noSessions') }}
		</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { roomKey, type RoomInfo } from '../types';
	import Avatar from './Avatar.vue';

	export default defineComponent({
		name: 'ManagerHierarchyTab',
		components: { Avatar },
		props: {
			rooms: { type: Array as PropType<RoomInfo[]>, required: true },
			activeKey: { type: String, default: null },
		},
		emits: ['select'],
		computed: {
			managers(): RoomInfo[] {
				return this.rooms.filter((r) => r.room.kind === 'manager');
			},
			groups(): { machine: string; sessions: RoomInfo[] }[] {
				const by = new Map<string, RoomInfo[]>();
				for (const r of this.rooms) {
					if (r.room.kind === 'manager') continue;
					by.set(r.room.machine, [...(by.get(r.room.machine) ?? []), r]);
				}
				return [...by].map(([machine, sessions]) => ({ machine, sessions }));
			},
		},
		methods: {
			keyOf: (info: RoomInfo) => roomKey(info.room),
		},
	});
</script>
