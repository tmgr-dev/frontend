<template>
	<section class="card">
		<div class="meta">
			<span class="tag" :class="'t-' + card.kind">{{
				KIND_LABELS[card.kind]
			}}</span>
			<span class="tag src">{{ sourceLabel(card) }}</span>
			<span v-if="card.session_gone" class="tag gone">Session ended</span>
			<span
				v-if="card.blocking && mode !== 'final' && !card.session_gone"
				class="tag blocking"
				>Agent is waiting</span
			>
			<button
				v-if="card.links.task"
				type="button"
				class="chip"
				@click="openLink(card.links.task)"
			>
				{{ linkLabel(card.links.task) }}
			</button>
			<button
				v-if="card.links.pr"
				type="button"
				class="chip"
				@click="openLink(card.links.pr)"
			>
				{{ linkLabel(card.links.pr) }}
			</button>
			<span>· {{ position }} of {{ total }}</span>
		</div>
		<h1>{{ card.title }}</h1>
		<div
			v-if="contextHtml"
			class="md"
			@click="onContextClick"
			v-html="contextHtml"
		></div>
		<div v-if="card.facts.length" class="facts">
			<div
				v-for="(fact, index) in card.facts"
				:key="index"
				class="fact"
				:class="fact.state"
			>
				<span class="ic">{{ FACT_GLYPH[fact.state] }}</span>
				<span>{{ fact.text }}</span>
				<button
					v-if="fact.link"
					type="button"
					class="chip"
					:title="fact.link"
					@click="openLink(fact.link)"
				>
					{{ linkLabel(fact.link) }}
				</button>
			</div>
		</div>

		<div v-if="mode === 'final'" class="notice final">
			<div class="what">{{ finalTitle }}</div>
			<div v-if="finalDetail">{{ finalDetail }}</div>
		</div>

		<template v-else>
			<div v-if="card.question" class="q">{{ card.question }}</div>

			<div v-if="mode === 'dismiss-only'" class="notice">
				This card comes from a terminal session without a return path. Answer in
				the agent session, or dismiss it here.
				<div style="margin-top: 10px">
					<button
						type="button"
						class="btn"
						@click="$emit('answer', { dismissed: true })"
					>
						Dismiss <kbd>X</kbd>
					</button>
				</div>
			</div>

			<template v-else>
				<div
					v-for="(option, index) in card.options"
					:key="option.id"
					class="optw"
				>
					<div
						class="opt"
						:class="{ sel: option.id === selected }"
						role="button"
						tabindex="0"
						@click="onOptionClick(index, $event)"
						@keydown.enter.self.prevent="pick(index, false)"
					>
						<kbd>{{ index + 1 }}</kbd>
						<span class="label">{{ option.label }}</span>
						<span v-if="option.recommended" class="star">★ recommended</span>
						<span v-else class="spacer"></span>
						<button
							type="button"
							class="add"
							title="Choose and add a note"
							@click.stop="pick(index, true)"
						>
							✎ + note
						</button>
					</div>
					<div v-if="noteFor === option.id" class="note">
						<textarea
							ref="note"
							v-model="note"
							data-input="note"
							:placeholder="
								'Note for “' + option.label + '”… sent with your choice'
							"
							@keydown="onNoteKey($event, option.id)"
						></textarea>
						<div class="tf">
							<span>Enter — send · Shift+Enter — new line</span>
							<span>Esc — without a note</span>
						</div>
					</div>
				</div>
				<div v-if="viaRunner" class="notice">
					Your answer is typed into the agent session.
				</div>
				<textarea
					ref="free"
					v-model="freeText"
					class="free"
					data-input="free"
					:placeholder="
						card.options.length
							? 'Your own answer… (T) — sent to the agent as a comment'
							: 'Your answer… (T) — Enter to send'
					"
					@keydown="onFreeKey"
				></textarea>
				<div class="hints">
					<span v-if="hasRecommended"><kbd>Enter</kbd> accept ★</span>
					<span v-if="card.options.length"
						><kbd>1</kbd>–<kbd>{{ card.options.length }}</kbd> pick</span
					>
					<span
						><kbd>⇧</kbd>+<kbd>1</kbd>… / <kbd>⇧Enter</kbd> pick + note</span
					>
					<span><kbd>D</kbd> defer</span>
					<span><kbd>X</kbd> dismiss</span>
					<span><kbd>C</kbd> chat</span>
					<span><kbd>↑</kbd><kbd>↓</kbd> cards</span>
				</div>
			</template>
		</template>
	</section>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { cardMode, type CardMode } from '../answer';
	import {
		KIND_LABELS,
		isHttpUrl,
		linkLabel,
		recommendedOption,
		sourceLabel,
	} from '../format';
	import { isSubmitKey } from '../keys';
	import type { CardView } from '../types';
	import { markdownToHtml } from '@/utils/markdown';
	import sanitizeHtml from '@/utils/sanitizeHtml';

	const PICK_DELAY_MS = 180;

	export default defineComponent({
		name: 'CardPane',
		props: {
			card: { type: Object as PropType<CardView>, required: true },
			position: { type: Number, required: true },
			total: { type: Number, required: true },
		},
		emits: ['answer', 'open-link'],
		data() {
			const recommended = recommendedOption(this.card);
			return {
				KIND_LABELS,
				FACT_GLYPH: { ok: '✓', bad: '✗', wait: '◷' },
				selected: (recommended?.id ?? null) as string | null,
				noteFor: null as string | null,
				note: '',
				freeText: '',
				pickTimer: 0 as number,
			};
		},
		computed: {
			mode(): CardMode {
				return cardMode(this.card);
			},
			hasRecommended(): boolean {
				return recommendedOption(this.card) !== null;
			},
			viaRunner(): boolean {
				return (
					this.card.reply_via === 'hook' &&
					Boolean(this.card.source.runner_session)
				);
			},
			contextHtml(): string {
				const text = this.card.context;
				return text
					? sanitizeHtml(markdownToHtml(text), { remoteMedia: false })
					: '';
			},
			finalTitle(): string {
				const { state, answer } = this.card;
				if (state === 'dismissed') return 'Dismissed';
				if (state === 'closed') return 'Closed';
				if (answer?.deferred) return 'Deferred';
				return 'Answered';
			},
			finalDetail(): string {
				const { state, answer, closed_reason: reason, options } = this.card;
				if (state === 'closed') return reason ?? '';
				if (!answer) return '';
				const option = options.find((o) => o.id === answer.choice);
				return [option?.label, answer.note, answer.free_text]
					.filter(Boolean)
					.join(' — ');
			},
		},
		beforeUnmount() {
			window.clearTimeout(this.pickTimer);
		},
		methods: {
			sourceLabel,
			linkLabel,
			openLink(url: string) {
				this.$emit('open-link', url);
			},
			onContextClick(event: MouseEvent) {
				const anchor = (event.target as HTMLElement).closest('a');
				if (!anchor) return;
				event.preventDefault();
				const href = anchor.getAttribute('href') ?? '';
				if (isHttpUrl(href)) this.openLink(href);
			},
			acceptRecommended(withNote: boolean) {
				const index = this.card.options.findIndex((o) => o.recommended);
				if (index >= 0) this.pick(index, withNote);
			},
			onOptionClick(index: number, event: MouseEvent) {
				this.pick(index, event.shiftKey);
			},
			pick(index: number, withNote: boolean) {
				const option = this.card.options[index];
				if (!option || this.mode !== 'answer') return;
				this.selected = option.id;
				if (withNote) {
					this.noteFor = option.id;
					void this.$nextTick(() => {
						const area = this.$refs.note as HTMLTextAreaElement[] | undefined;
						area?.[0]?.focus();
					});
					return;
				}
				this.noteFor = null;
				window.clearTimeout(this.pickTimer);
				this.pickTimer = window.setTimeout(
					() => this.$emit('answer', { choice: option.id }),
					PICK_DELAY_MS,
				);
			},
			focusFree() {
				(this.$refs.free as HTMLTextAreaElement | undefined)?.focus();
			},
			onNoteKey(event: KeyboardEvent, optionId: string) {
				if (event.key === 'Escape') {
					this.noteFor = null;
					this.note = '';
					return;
				}
				if (isSubmitKey(event)) {
					event.preventDefault();
					this.$emit('answer', { choice: optionId, note: this.note });
				}
			},
			onFreeKey(event: KeyboardEvent) {
				if (isSubmitKey(event) && this.freeText.trim()) {
					event.preventDefault();
					this.$emit('answer', { freeText: this.freeText });
				}
			},
		},
	});
</script>
