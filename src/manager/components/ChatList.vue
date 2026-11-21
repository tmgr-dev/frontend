<template>
	<div class="sl" role="listbox" :aria-label="$t('manager.list.label')">
		<template v-if="pinned.length > 0">
			<div class="gl">{{ $t('manager.list.pinned') }}</div>
			<button
				v-for="info in pinned"
				:key="keyOf(info)"
				type="button"
				role="option"
				:aria-selected="keyOf(info) === activeKey"
				:class="['crow', { on: keyOf(info) === activeKey }]"
				@click="$emit('select', info.room)"
			>
				<Avatar manager />
				<span class="cm2">
					<span class="cn"
						><b>{{ info.title }}</b
						><i>{{ time(info) }}</i></span
					>
					<span class="cp">{{ preview(info) }}</span>
					<span class="cmeta"
						><span class="lockb"
							><Icon name="lock" />{{ $t('manager.header.encrypted') }}</span
						></span
					>
				</span>
				<span v-if="badge(info)" class="ub">{{ badge(info) }}</span>
			</button>
		</template>
		<template v-if="sessions.length > 0">
			<div class="gl">{{ $t('manager.list.sessions') }}</div>
			<button
				v-for="info in sessions"
				:key="keyOf(info)"
				type="button"
				role="option"
				:aria-selected="keyOf(info) === activeKey"
				:class="[
					'crow',
					`st-${info.state ?? 'idle'}`,
					{ on: keyOf(info) === activeKey },
				]"
				@click="$emit('select', info.room)"
			>
				<Avatar :agent="info.agent" />
				<span class="cm2">
					<span class="cn"
						><b>{{ info.title }}</b
						><i>{{ time(info) }}</i></span
					>
					<span class="cp">{{ preview(info) }}</span>
					<span class="cmeta">
						<StatePill v-if="info.state" :state="info.state" />
						<ContextMeter
							v-if="info.context_tokens !== undefined"
							:tokens="info.context_tokens"
							:pct="info.context_pct"
						/>
					</span>
				</span>
				<span v-if="badge(info)" class="ub">{{ badge(info) }}</span>
			</button>
		</template>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { roomKey, type RoomInfo } from '../types';
	import Avatar from './Avatar.vue';
	import ContextMeter from './ContextMeter.vue';
	import Icon from './Icon.vue';
	import StatePill from './StatePill.vue';
	import { formatClock, plainPreview } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerChatList',
		components: { Avatar, ContextMeter, Icon, StatePill },
		props: {
			rooms: { type: Array as PropType<RoomInfo[]>, required: true },
			activeKey: { type: String, default: null },
			unread: {
				type: Object as PropType<Record<string, number>>,
				default: () => ({}),
			},
		},
		emits: ['select'],
		computed: {
			pinned(): RoomInfo[] {
				return this.rooms.filter((r) => r.room.kind === 'manager');
			},
			sessions(): RoomInfo[] {
				return this.rooms.filter((r) => r.room.kind !== 'manager');
			},
		},
		methods: {
			keyOf(info: RoomInfo): string {
				return roomKey(info.room);
			},
			time(info: RoomInfo): string {
				return info.last ? formatClock(info.last.ts_ms) : '';
			},
			preview(info: RoomInfo): string {
				if (!info.last) return this.$t('manager.list.noMessages');
				const own =
					info.last.author.kind === 'owner' ? this.$t('manager.list.you') : '';
				return own + plainPreview(info.last.body_md);
			},
			badge(info: RoomInfo): number {
				return this.unread[roomKey(info.room)] ?? 0;
			},
		},
	});
</script>
