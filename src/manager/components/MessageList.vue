<template>
	<div class="msgs">
		<div
			ref="scroll"
			class="scroll"
			role="log"
			tabindex="-1"
			:aria-label="$t('manager.messages.label')"
			@scroll.passive="onScroll"
		>
			<div ref="inner" class="inner">
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
						:queued="queuedIds.includes(row.message.id)"
						:tools="toolsByMessage[row.message.id]"
						@reply="$emit('reply', $event)"
						@copy="$emit('copy', $event)"
						@cancel-queued="$emit('cancel-queued', $event)"
					>
						<template #decision="slotProps">
							<slot name="decision" :message="slotProps.message"></slot>
						</template>
					</MessageBubble>
					<MentionReply
						v-if="mentionOf(row.message)"
						:message="row.message"
						:latest="mentionOf(row.message)!"
						:origin="origin"
						:busy="!!mentionBusy[row.message.mention!.id]"
						:injected="!!mentionDone[row.message.mention!.id]"
						@open="$emit('open-room', $event)"
						@inject="$emit('inject', $event)"
					/>
				</template>
				<div v-if="working" class="live" role="status" aria-live="polite">
					<span class="live-dot" aria-hidden="true"></span>
					<span class="live-text">{{
						activity?.current ?? $t('manager.activity.working')
					}}</span>
				</div>
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
	import type {
		ActivityView,
		ChatMessage,
		MentionRef,
		RoomInfo,
	} from '../types';
	import {
		anchoredScrollTop,
		arrivalsAfter,
		isNearBottom,
		newBelowCount,
		shouldLoadOlder,
	} from '../scrollLogic';
	import { isMentionStateRow, latestMentions, mentionRole } from '../mentions';
	import Icon from './Icon.vue';
	import MentionReply from './MentionReply.vue';
	import MessageBubble from './MessageBubble.vue';
	import { dayKey, formatDay, isGrouped, isOwnerMessage } from './viewHelpers';

	interface Row {
		message: ChatMessage;
		day?: string;
		replyTo?: ChatMessage;
		grouped: boolean;
	}

	const isOwn = isOwnerMessage;

	export default defineComponent({
		name: 'ManagerMessageList',
		components: { Icon, MentionReply, MessageBubble },
		props: {
			messages: { type: Array as PropType<ChatMessage[]>, required: true },
			bodyHtml: {
				type: Object as PropType<Record<string, string>>,
				default: () => ({}),
			},
			now: { type: Number, default: () => Date.now() },
			emptyHint: { type: String, default: undefined },
			queuedIds: {
				type: Array as PropType<string[]>,
				default: () => [],
			},
			toolsByMessage: {
				type: Object as PropType<Record<string, string[]>>,
				default: () => ({}),
			},
			activity: {
				type: Object as PropType<ActivityView | null>,
				default: null,
			},
			roomKey: { type: String, default: '' },
			shownRooms: {
				type: Object as PropType<Set<string>>,
				default: () => new Set<string>(),
			},
			origin: { type: Object as PropType<RoomInfo>, default: undefined },
			mentionBusy: {
				type: Object as PropType<Record<string, boolean>>,
				default: () => ({}),
			},
			mentionDone: {
				type: Object as PropType<Record<string, boolean>>,
				default: () => ({}),
			},
			loadOlder: {
				type: Function as PropType<() => Promise<boolean | null>>,
				default: undefined,
			},
		},
		emits: ['reply', 'copy', 'cancel-queued', 'open-room', 'inject'],
		data() {
			return {
				pinned: true,
				newBelow: 0,
				trackedKey: '',
				gen: 0,
				lastId: undefined as string | undefined,
				firstId: undefined as string | undefined,
				pendingBottom: false,
				olderBusy: false,
				olderDone: false,
				jumping: false,
				jumpTimer: 0,
				views: new Map<string, { scrollTop: number; pinned: boolean }>(),
				observer: null as ResizeObserver | null,
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
			if (typeof ResizeObserver !== 'undefined') {
				this.observer = new ResizeObserver(() => {
					if (this.pinned) this.pinEnd();
				});
				this.observer.observe(this.$refs.inner as Element);
			}
			this.sync();
		},
		beforeUnmount() {
			this.observer?.disconnect();
			clearTimeout(this.jumpTimer);
		},
		methods: {
			mentionOf(message: ChatMessage): MentionRef | undefined {
				const m = message.mention;
				if (!m || !mentionRole(message) || mentionRole(message) === 'state')
					return undefined;
				return this.latestMention.get(m.id) ?? m;
			},
			sync() {
				const list = this.messages;
				if (this.roomKey !== this.trackedKey) this.switchRoom(list);
				const tail = arrivalsAfter(this.lastId, list);
				const prepended =
					this.firstId !== undefined && list[0]?.id !== this.firstId;
				const before = prepended
					? (this.$refs.scroll as HTMLElement).scrollHeight
					: 0;
				this.lastId = list[list.length - 1]?.id;
				this.firstId = list[0]?.id;
				if (this.pendingBottom) {
					if (list.length) {
						this.pendingBottom = false;
						this.shownRooms.add(this.roomKey);
						this.pinEnd();
					}
					return;
				}
				if (!tail.length && !prepended) return;
				if (this.pinned || tail.some(isOwn)) {
					this.pinEnd();
					return;
				}
				const arrived = tail.filter((m) => !isOwn(m)).length;
				this.newBelow = newBelowCount(this.newBelow, arrived, this.pinned);
				if (prepended) this.anchorOlder(before);
			},
			switchRoom(list: ChatMessage[]) {
				const el = this.$refs.scroll as HTMLElement | undefined;
				if (el && this.trackedKey) {
					this.views.set(this.trackedKey, {
						scrollTop: el.scrollTop,
						pinned: this.pinned,
					});
				}
				const saved = this.views.get(this.roomKey);
				this.enterRoom(list);
				if (this.pendingBottom || !saved) return;
				this.$nextTick(() => {
					const scroller = this.$refs.scroll as HTMLElement | undefined;
					if (!scroller) return;
					scroller.scrollTop = saved.pinned
						? scroller.scrollHeight
						: saved.scrollTop;
					this.pinned = isNearBottom(scroller);
				});
			},
			enterRoom(list: ChatMessage[]) {
				this.endJump();
				this.trackedKey = this.roomKey;
				this.gen += 1;
				this.olderDone = false;
				this.olderBusy = false;
				this.newBelow = 0;
				this.lastId = list[list.length - 1]?.id;
				this.firstId = list[0]?.id;
				this.pendingBottom = !this.shownRooms.has(this.roomKey);
			},
			pinEnd() {
				this.pinned = true;
				this.newBelow = 0;
				this.$nextTick(() => {
					const el = this.$refs.scroll as HTMLElement | undefined;
					if (el) el.scrollTop = el.scrollHeight;
				});
			},
			anchorOlder(before: number) {
				this.$nextTick(() => {
					const el = this.$refs.scroll as HTMLElement | undefined;
					if (el)
						el.scrollTop = anchoredScrollTop(
							el.scrollTop,
							before,
							el.scrollHeight,
						);
				});
			},
			jumpToEnd() {
				const el = this.$refs.scroll as HTMLElement;
				this.jumping = true;
				clearTimeout(this.jumpTimer);
				this.jumpTimer = window.setTimeout(() => this.endJump(), 600);
				this.pinned = true;
				this.newBelow = 0;
				el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
				el.focus({ preventScroll: true });
			},
			endJump() {
				this.jumping = false;
				clearTimeout(this.jumpTimer);
			},
			onScroll(e: Event) {
				const el = e.target as HTMLElement;
				const near = isNearBottom(el);
				if (this.jumping) {
					if (near) this.endJump();
					return;
				}
				this.pinned = near;
				if (near) this.newBelow = 0;
				if (
					this.loadOlder &&
					shouldLoadOlder(el.scrollTop, this.olderBusy, this.olderDone)
				) {
					void this.fetchOlder(this.loadOlder);
				}
			},
			async fetchOlder(loadOlder: () => Promise<boolean | null>) {
				const gen = this.gen;
				this.olderBusy = true;
				let grew: boolean | null;
				try {
					grew = await loadOlder();
				} catch {
					grew = null;
				}
				if (gen !== this.gen) return;
				if (grew === false) this.olderDone = true;
				this.olderBusy = false;
			},
		},
		computed: {
			working(): boolean {
				return this.activity?.state === 'working';
			},
			latestMention(): Map<string, MentionRef> {
				return latestMentions(this.messages);
			},
			rows(): Row[] {
				const byId = new Map(this.messages.map((m) => [m.id, m]));
				const labels = {
					today: this.$t('manager.messages.today'),
					yesterday: this.$t('manager.messages.yesterday'),
				};
				const shown = this.messages.filter((m) => !isMentionStateRow(m));
				return shown.map((message, i) => {
					const prev = shown[i - 1];
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
