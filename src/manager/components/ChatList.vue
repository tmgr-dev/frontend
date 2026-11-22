<template>
	<div
		class="sl"
		role="listbox"
		:aria-label="$t('manager.list.label')"
		@mouseenter="onEnter"
		@mouseleave="onLeave"
	>
		<template v-for="(g, gi) in view" :key="g.key">
			<div v-if="g.header" class="gl gh">{{ g.header }}</div>
			<template v-if="g.pinned.length > 0">
				<div v-if="!g.header" class="gl">{{ $t('manager.list.pinned') }}</div>
				<button
					v-for="info in g.pinned"
					:key="keyOf(info)"
					type="button"
					role="option"
					:aria-selected="keyOf(info) === activeKey"
					:class="['crow', { on: keyOf(info) === activeKey }]"
					@click="$emit('select', keyOf(info))"
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
			<template v-if="g.sessions.length > 0">
				<div v-if="!g.header || gi === 0" class="gl glsort">
					<span>{{ $t('manager.list.sessions') }}</span>
					<button
						type="button"
						class="sortb"
						:aria-pressed="mode === 'recent'"
						:title="$t('manager.list.sortHint')"
						@click="toggleSort"
					>
						{{
							mode === 'recent'
								? $t('manager.list.sortRecent')
								: $t('manager.list.sortDefault')
						}}
					</button>
				</div>
				<button
					v-for="info in g.sessions"
					:key="keyOf(info)"
					type="button"
					role="option"
					:aria-selected="keyOf(info) === activeKey"
					:class="[
						'crow',
						`st-${info.state ?? 'idle'}`,
						{ on: keyOf(info) === activeKey },
					]"
					@click="$emit('select', keyOf(info))"
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
		</template>
		<p v-if="sessions.length === 0" class="mgr-hint">
			{{ $t('manager.list.noSessions') }}
		</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { groupRoomsByMachine } from '../machineTree';
	import {
		arrangeByKeys,
		loadSortMode,
		saveSortMode,
		sortRooms,
		type SortMode,
	} from '../roomOrder';
	import { roomKey, type ManagerMachine, type RoomInfo } from '../types';
	import Avatar from './Avatar.vue';
	import ContextMeter from './ContextMeter.vue';
	import Icon from './Icon.vue';
	import StatePill from './StatePill.vue';
	import { formatClock, plainPreview } from './viewHelpers';

	interface ChatGroup {
		key: string;
		header: string | null;
		pinned: RoomInfo[];
		sessions: RoomInfo[];
	}

	export default defineComponent({
		name: 'ManagerChatList',
		components: { Avatar, ContextMeter, Icon, StatePill },
		props: {
			rooms: { type: Array as PropType<RoomInfo[]>, required: true },
			machines: {
				type: Array as PropType<ManagerMachine[]>,
				default: () => [],
			},
			activeKey: { type: String, default: null },
			unread: {
				type: Object as PropType<Record<string, number>>,
				default: () => ({}),
			},
		},
		emits: ['select'],
		data() {
			return {
				mode: loadSortMode() as SortMode,
				hover: false,
				keys: [] as string[],
				timer: null as ReturnType<typeof setTimeout> | null,
			};
		},
		computed: {
			pinned(): RoomInfo[] {
				return this.rooms.filter((r) => r.room.kind === 'manager');
			},
			target(): string[] {
				const all = this.rooms.filter((r) => r.room.kind !== 'manager');
				return sortRooms(all, this.mode).map((r) => roomKey(r.room));
			},
			sessions(): RoomInfo[] {
				const all = this.rooms.filter((r) => r.room.kind !== 'manager');
				return arrangeByKeys(all, this.keys);
			},
			view(): ChatGroup[] {
				const groups = groupRoomsByMachine(
					[...this.pinned, ...this.sessions],
					this.machines,
				);
				if (!groups)
					return [
						{
							key: '',
							header: null,
							pinned: this.pinned,
							sessions: this.sessions,
						},
					];
				return groups.map((g) => ({
					key: g.machine,
					header: g.main
						? this.$t('manager.list.mainMachine', {
								label: g.label ?? this.$t('manager.list.thisComputer'),
							})
						: this.$t('manager.list.subMachine', { label: g.label }),
					pinned: g.rooms.filter((r) => r.room.kind === 'manager'),
					sessions: g.rooms.filter((r) => r.room.kind !== 'manager'),
				}));
			},
		},
		watch: {
			target() {
				this.schedule();
			},
		},
		created() {
			this.keys = this.target;
		},
		beforeUnmount() {
			if (this.timer) clearTimeout(this.timer);
		},
		methods: {
			schedule() {
				if (this.timer) clearTimeout(this.timer);
				if (this.hover) return;
				this.timer = setTimeout(() => {
					this.keys = this.target;
				}, 600);
			},
			onEnter() {
				this.hover = true;
				if (this.timer) clearTimeout(this.timer);
			},
			onLeave() {
				this.hover = false;
				this.schedule();
			},
			toggleSort() {
				this.mode = this.mode === 'recent' ? 'default' : 'recent';
				saveSortMode(this.mode);
				this.keys = this.target;
			},
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
