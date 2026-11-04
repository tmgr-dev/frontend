<template>
	<div>
		<div
			class="titlebar"
			:class="{ 'no-lights': !isMac }"
			data-tauri-drag-region
		>
			<div class="lights" data-tauri-drag-region></div>
			<div class="wtitle" data-tauri-drag-region>
				tmgr · Decisions <span>— tmgr.dev</span>
			</div>
			<div class="balance" data-tauri-drag-region></div>
		</div>
		<div class="shell">
			<QueueList
				:groups="groups"
				:selected-id="selectedId"
				:progress="progress"
				@select="select"
				@dismiss="closeCard"
			/>
			<main class="canvas">
				<template v-if="current">
					<CardPane
						ref="cardPane"
						:key="current.id"
						:card="current"
						:position="position"
						:total="order.length"
						@answer="submit"
						@open-link="openLink"
					/>
					<section class="right">
						<div class="tabs" role="tablist">
							<button
								type="button"
								:class="{ on: tab === 'diff' }"
								@click="tab = 'diff'"
							>
								Changes
								<span v-if="threadCount" class="cnt">{{ threadCount }}</span>
							</button>
							<button
								type="button"
								:class="{ on: tab === 'checks' }"
								@click="tab = 'checks'"
							>
								Checks
								<span v-if="current.checks.length" class="cnt">{{
									current.checks.length
								}}</span>
							</button>
							<button
								type="button"
								:class="{ on: tab === 'chat' }"
								@click="openChat"
							>
								Chat <kbd>C</kbd>
							</button>
							<button
								v-if="tab === 'diff' && currentHasChanges"
								type="button"
								class="refresh"
								@click="loadDiff(current.id, true)"
							>
								Refresh
							</button>
						</div>
						<DiffPane
							v-if="tab === 'diff'"
							:key="'d' + current.id"
							:diff="currentDiff"
							:has-diff="currentHasChanges"
							:messages="current.messages"
							:drafts="currentDrafts"
							:can-reply="current.can_reply"
							:final="isFinal(current)"
							@send="sendMessage"
							@add-draft="addDraft"
							@remove-draft="removeDraft"
						/>
						<ChecksPane v-else-if="tab === 'checks'" :checks="current.checks" />
						<ChatPane
							v-else
							ref="chatPane"
							:key="'c' + current.id"
							:messages="chatMessages"
							:can-reply="current.can_reply && !isFinal(current)"
							@send="sendChat"
						/>
					</section>
				</template>
				<div v-else class="empty">
					<div class="big">✓</div>
					{{ loaded ? 'All clear' : 'Loading…' }}
					<small v-if="loaded">Nothing is waiting for your decision.</small>
				</div>
			</main>
		</div>
		<footer class="statusbar">
			<span>tmgr.dev</span><span>›</span>
			<span style="color: var(--fg)">Decisions</span>
			<span class="grow"></span>
			<div class="keys">
				<span><kbd>Enter</kbd> accept</span>
				<span><kbd>1</kbd>–<kbd>9</kbd> pick</span>
				<span><kbd>⇧</kbd> + note</span>
				<span><kbd>T</kbd> free text</span>
				<span><kbd>D</kbd> defer</span>
				<span><kbd>X</kbd> dismiss</span>
				<span><kbd>↑</kbd><kbd>↓</kbd> cards</span>
			</div>
			<span class="dot"></span>
			<span>{{ progress.pending }} waiting</span>
		</footer>
		<div class="toast" :class="{ on: toast.on, error: toast.error }">
			<span>{{ toast.text }}</span>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import {
		decisionsAnswer,
		decisionsDiff,
		decisionsGet,
		decisionsList,
		decisionsMessage,
		decisionsOpenLink,
		onDecisionsChanged,
	} from '../api';
	import {
		answerToast,
		buildAnswer,
		cardMode,
		isFinal,
		type AnswerDraft,
	} from '../answer';
	import { hasChanges, splitMessages } from '../diff';
	import { mapKey } from '../keys';
	import {
		buildQueue,
		flattenQueue,
		initialCardId,
		nextCardId,
		queueProgress,
		stepCardId,
		visibleCards,
	} from '../queue';
	import type {
		CardView,
		DiffGroup,
		LineComment,
		OutgoingMessage,
	} from '../types';
	import { isHttpUrl, recommendedOption } from '../format';
	import CardPane from './CardPane.vue';
	import ChatPane from './ChatPane.vue';
	import ChecksPane from './ChecksPane.vue';
	import DiffPane from './DiffPane.vue';
	import QueueList from './QueueList.vue';

	interface DiffSlot {
		status: 'loading' | 'ready' | 'error';
		groups?: DiffGroup[];
		error?: string;
		stamp?: number;
	}

	type Tab = 'diff' | 'checks' | 'chat';

	export default defineComponent({
		name: 'DecisionsApp',
		components: { CardPane, ChatPane, ChecksPane, DiffPane, QueueList },
		data() {
			return {
				cards: [] as CardView[],
				resolvedHere: [] as string[],
				selectedId: null as string | null,
				tab: 'diff' as Tab,
				loaded: false,
				diffs: {} as Record<string, DiffSlot>,
				drafts: {} as Record<string, LineComment[]>,
				toast: { on: false, error: false, text: '' },
				toastTimer: 0 as number,
				unlisten: null as null | (() => void),
				refreshing: false,
				refreshAgain: false,
				isMac: /Mac/i.test(navigator.userAgent),
			};
		},
		computed: {
			visible(): CardView[] {
				return visibleCards(this.cards, new Set(this.resolvedHere));
			},
			groups() {
				return buildQueue(this.visible);
			},
			order(): CardView[] {
				return flattenQueue(this.groups);
			},
			progress() {
				return queueProgress(this.visible);
			},
			current(): CardView | null {
				return this.order.find((card) => card.id === this.selectedId) ?? null;
			},
			position(): number {
				return this.order.findIndex((card) => card.id === this.selectedId) + 1;
			},
			currentHasChanges(): boolean {
				return this.current ? hasChanges(this.current) : false;
			},
			currentDiff(): DiffSlot | null {
				return this.current ? (this.diffs[this.current.id] ?? null) : null;
			},
			currentDrafts(): LineComment[] {
				return this.current ? (this.drafts[this.current.id] ?? []) : [];
			},
			chatMessages() {
				return this.current ? splitMessages(this.current.messages).chat : [];
			},
			threadCount(): number {
				if (!this.current) return 0;
				const keys = new Set(
					splitMessages(this.current.messages).threads.keys(),
				);
				for (const draft of this.currentDrafts) {
					keys.add(`${draft.file}:${draft.line}`);
				}
				return keys.size;
			},
		},
		watch: {
			order() {
				this.ensureSelection();
			},
			selectedId() {
				this.ensureDiff();
			},
			currentHasChanges() {
				this.ensureDiff();
			},
			'current.updated_ms'() {
				this.ensureDiff();
			},
		},
		async mounted() {
			window.addEventListener('keydown', this.onKeydown);
			document.title = 'Decisions';
			try {
				this.unlisten = await onDecisionsChanged(() => void this.refresh());
			} catch {
				this.unlisten = null;
			}
			await this.refresh();
		},
		beforeUnmount() {
			window.removeEventListener('keydown', this.onKeydown);
			window.clearTimeout(this.toastTimer);
			this.unlisten?.();
		},
		methods: {
			isFinal,
			async refresh() {
				if (this.refreshing) {
					this.refreshAgain = true;
					return;
				}
				this.refreshing = true;
				try {
					this.cards = await decisionsList();
				} catch (error) {
					this.showToast(`Could not load decisions: ${String(error)}`, true);
				} finally {
					this.loaded = true;
					this.refreshing = false;
				}
				if (this.refreshAgain) {
					this.refreshAgain = false;
					await this.refresh();
				}
			},
			async refreshCard(id: string) {
				try {
					const card = await decisionsGet(id);
					const index = this.cards.findIndex((c) => c.id === id);
					if (index >= 0) this.cards.splice(index, 1, card);
				} catch {
					await this.refresh();
				}
			},
			ensureSelection() {
				if (this.order.some((card) => card.id === this.selectedId)) return;
				this.selectedId = initialCardId(this.order);
			},
			ensureDiff() {
				const card = this.current;
				if (!card || !hasChanges(card)) return;
				const slot = this.diffs[card.id];
				void this.loadDiff(card.id, !!slot && slot.stamp !== card.updated_ms);
			},
			async loadDiff(id: string, force: boolean) {
				if (!force && this.diffs[id]) return;
				const stamp = this.cards.find((c) => c.id === id)?.updated_ms;
				if (!this.diffs[id]) this.diffs[id] = { status: 'loading' };
				try {
					const { groups } = await decisionsDiff(id);
					this.diffs[id] = { status: 'ready', groups, stamp };
				} catch (error) {
					this.diffs[id] = { status: 'error', error: String(error), stamp };
				}
			},
			select(id: string) {
				this.selectedId = id;
			},
			showToast(text: string, error = false) {
				window.clearTimeout(this.toastTimer);
				this.toast = { on: true, error, text };
				this.toastTimer = window.setTimeout(() => {
					this.toast.on = false;
				}, 1800);
			},
			openLink(url: string) {
				if (isHttpUrl(url)) void decisionsOpenLink(url).catch(() => undefined);
			},
			openChat() {
				this.tab = 'chat';
				void this.$nextTick(() => {
					(
						this.$refs.chatPane as { focus?: () => void } | undefined
					)?.focus?.();
				});
			},
			async submit(draft: AnswerDraft) {
				const card = this.current;
				if (!card) return;
				const answer = buildAnswer({
					...draft,
					lineComments: this.drafts[card.id] ?? [],
				});
				try {
					await decisionsAnswer(card.id, answer);
				} catch (error) {
					this.showToast(`Could not send the answer: ${String(error)}`, true);
					return;
				}
				const following = nextCardId(this.order, card.id);
				if (!this.resolvedHere.includes(card.id))
					this.resolvedHere.push(card.id);
				delete this.drafts[card.id];
				this.showToast(answerToast(card, answer));
				this.selectedId = following;
				await this.refresh();
				this.ensureSelection();
			},
			async closeCard(id: string) {
				const card = this.cards.find((c) => c.id === id);
				if (!card) return;
				const answer = buildAnswer({ dismissed: true });
				try {
					await decisionsAnswer(id, answer);
				} catch (error) {
					this.showToast(`Could not send the answer: ${String(error)}`, true);
					return;
				}
				const following = nextCardId(this.order, id);
				if (!this.resolvedHere.includes(id)) this.resolvedHere.push(id);
				delete this.drafts[id];
				this.showToast(answerToast(card, answer));
				if (this.selectedId === id) this.selectedId = following;
				await this.refresh();
				this.ensureSelection();
			},
			async sendMessage(message: OutgoingMessage) {
				const card = this.current;
				if (!card) return;
				try {
					await decisionsMessage(card.id, message);
					await this.refreshCard(card.id);
					if (message.action === 'fix') await this.loadDiff(card.id, true);
				} catch (error) {
					this.showToast(`Could not send the message: ${String(error)}`, true);
				}
			},
			sendChat(text: string) {
				void this.sendMessage({ text, action: 'reply' });
			},
			addDraft(comment: LineComment) {
				const card = this.current;
				if (!card) return;
				const list = this.drafts[card.id] ?? [];
				this.drafts[card.id] = [...list, comment];
			},
			removeDraft(index: number) {
				const card = this.current;
				if (!card) return;
				this.drafts[card.id] = (this.drafts[card.id] ?? []).filter(
					(_, i) => i !== index,
				);
			},
			onKeydown(event: KeyboardEvent) {
				const target = document.activeElement as HTMLElement | null;
				const typing =
					!!target &&
					(target.tagName === 'TEXTAREA' ||
						target.tagName === 'INPUT' ||
						target.isContentEditable);
				const card = this.current;
				const mode = card ? cardMode(card) : 'final';
				const action = mapKey(event, {
					typing,
					optionCount: card?.options.length ?? 0,
					hasRecommended: card ? recommendedOption(card) !== null : false,
					canAnswer: mode === 'answer',
					canDismiss: mode !== 'final',
				});
				if (!action) return;
				const pane = this.$refs.cardPane as
					| {
							acceptRecommended: (withNote: boolean) => void;
							pick: (index: number, withNote: boolean) => void;
							focusFree: () => void;
					  }
					| undefined;
				event.preventDefault();
				switch (action.type) {
					case 'blur':
						target?.blur();
						break;
					case 'accept':
						pane?.acceptRecommended(action.withNote);
						break;
					case 'pick':
						pane?.pick(action.index, action.withNote);
						break;
					case 'free':
						pane?.focusFree();
						break;
					case 'defer':
						void this.submit({ deferred: true });
						break;
					case 'dismiss':
						void this.submit({ dismissed: true });
						break;
					case 'chat':
						this.openChat();
						break;
					case 'next':
					case 'prev': {
						const id = stepCardId(
							this.order,
							this.selectedId,
							action.type === 'next' ? 1 : -1,
						);
						if (id) this.selectedId = id;
						break;
					}
				}
			},
		},
	});
</script>
