<template>
	<div class="msgs">
		<div
			ref="scroll"
			class="scroll"
			role="log"
			:aria-label="$t('manager.messages.label')"
			@scroll.passive="onScroll"
		>
			<div class="inner">
				<div v-if="messages.length === 0" class="empty">
					<Icon name="term" /><b>{{ $t('manager.messages.empty') }}</b
					><span>{{ emptyHint ?? $t('manager.messages.emptyHint') }}</span>
				</div>
				<template v-for="row in rows" :key="row.message.id">
					<div v-if="row.day" class="day">{{ row.day }}</div>
					<MessageBubble
						:message="row.message"
						:html="bodyHtml[row.message.id]"
						:reply-to="row.replyTo"
						:grouped="row.grouped"
						@reply="$emit('reply', $event)"
						@copy="$emit('copy', $event)"
					>
						<template #decision="slotProps">
							<slot name="decision" :message="slotProps.message"></slot>
						</template>
					</MessageBubble>
				</template>
			</div>
		</div>
		<button
			v-if="!pinned && newBelow > 0"
			type="button"
			class="jump"
			:aria-label="$t('manager.messages.newBelow', newBelow)"
			@click="jumpToEnd"
		>
			<span aria-hidden="true">↓</span> {{ newBelow }}
		</button>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { ChatMessage } from '../types';
	import {
		anchoredScrollTop,
		arrivalsAfter,
		isNearBottom,
		newBelowCount,
		shouldLoadOlder,
	} from '../scrollLogic';
	import Icon from './Icon.vue';
	import MessageBubble from './MessageBubble.vue';
	import { dayKey, formatDay, isGrouped } from './viewHelpers';

	interface Row {
		message: ChatMessage;
		day?: string;
		replyTo?: ChatMessage;
		grouped: boolean;
	}

	const isOwn = (message: ChatMessage) => message.author.kind === 'owner';

	export default defineComponent({
		name: 'ManagerMessageList',
		components: { Icon, MessageBubble },
		props: {
			messages: { type: Array as PropType<ChatMessage[]>, required: true },
			bodyHtml: {
				type: Object as PropType<Record<string, string>>,
				default: () => ({}),
			},
			now: { type: Number, default: () => Date.now() },
			emptyHint: { type: String, default: undefined },
			roomKey: { type: String, default: '' },
			shownRooms: {
				type: Object as PropType<Set<string>>,
				default: () => new Set<string>(),
			},
			loadOlder: {
				type: Function as PropType<() => Promise<boolean>>,
				default: undefined,
			},
		},
		emits: ['reply', 'copy'],
		data() {
			return {
				pinned: true,
				newBelow: 0,
				trackedKey: '',
				lastId: undefined as string | undefined,
				pendingBottom: false,
				olderBusy: false,
				olderDone: false,
			};
		},
		watch: {
			messages() {
				this.sync();
			},
			roomKey() {
				this.sync();
			},
		},
		mounted() {
			this.sync();
		},
		methods: {
			sync() {
				const list = this.messages;
				if (this.roomKey !== this.trackedKey) this.enterRoom(list);
				const tail = arrivalsAfter(this.lastId, list);
				this.lastId = list[list.length - 1]?.id;
				if (this.olderBusy) return;
				if (this.pendingBottom) {
					if (list.length) {
						this.pendingBottom = false;
						this.pinEnd();
					}
					return;
				}
				if (!tail.length) return;
				if (this.pinned || tail.some(isOwn)) {
					this.pinEnd();
					return;
				}
				const arrived = tail.filter((m) => !isOwn(m)).length;
				this.newBelow = newBelowCount(this.newBelow, arrived, this.pinned);
			},
			enterRoom(list: ChatMessage[]) {
				this.trackedKey = this.roomKey;
				this.olderDone = false;
				this.olderBusy = false;
				this.newBelow = 0;
				this.lastId = list[list.length - 1]?.id;
				if (this.shownRooms.has(this.roomKey)) {
					this.pendingBottom = false;
					return;
				}
				this.shownRooms.add(this.roomKey);
				this.pendingBottom = true;
			},
			pinEnd() {
				this.pinned = true;
				this.newBelow = 0;
				this.$nextTick(() => {
					const el = this.$refs.scroll as HTMLElement | undefined;
					if (el) el.scrollTop = el.scrollHeight;
				});
			},
			jumpToEnd() {
				const el = this.$refs.scroll as HTMLElement;
				this.pinned = true;
				this.newBelow = 0;
				el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
			},
			onScroll(e: Event) {
				const el = e.target as HTMLElement;
				this.pinned = isNearBottom(el);
				if (this.pinned) this.newBelow = 0;
				if (
					this.loadOlder &&
					shouldLoadOlder(el.scrollTop, this.olderBusy, this.olderDone)
				) {
					void this.fetchOlder(el, this.loadOlder);
				}
			},
			async fetchOlder(el: HTMLElement, loadOlder: () => Promise<boolean>) {
				const key = this.trackedKey;
				const before = el.scrollHeight;
				this.olderBusy = true;
				try {
					const grew = await loadOlder();
					if (key !== this.trackedKey) return;
					if (!grew) {
						this.olderDone = true;
						return;
					}
					await this.$nextTick();
					el.scrollTop = anchoredScrollTop(
						el.scrollTop,
						before,
						el.scrollHeight,
					);
				} finally {
					if (key === this.trackedKey) this.olderBusy = false;
				}
			},
		},
		computed: {
			rows(): Row[] {
				const byId = new Map(this.messages.map((m) => [m.id, m]));
				const labels = {
					today: this.$t('manager.messages.today'),
					yesterday: this.$t('manager.messages.yesterday'),
				};
				return this.messages.map((message, i) => {
					const prev = this.messages[i - 1];
					const newDay = !prev || dayKey(prev.ts_ms) !== dayKey(message.ts_ms);
					return {
						message,
						day: newDay
							? formatDay(message.ts_ms, this.now, labels)
							: undefined,
						replyTo: message.reply_to ? byId.get(message.reply_to) : undefined,
						grouped: isGrouped(prev, message),
					};
				});
			},
		},
	});
</script>
