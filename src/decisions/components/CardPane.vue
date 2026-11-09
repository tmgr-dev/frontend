<template>
	<section class="card">
		<div class="meta">
			<span class="tag" :class="'t-' + card.kind">{{
				kindLabel(card.kind)
			}}</span>
			<span class="tag src">{{ sourceLabel(card) }}</span>
			<span v-if="card.session_gone" class="tag gone">{{
				$t('decisions.card.sessionEnded')
			}}</span>
			<span
				v-if="card.blocking && mode !== 'final' && !card.session_gone"
				class="tag blocking"
				>{{ $t('decisions.card.agentWaiting') }}</span
			>
			<span
				v-if="listen"
				class="tag listen"
				:class="{ on: card.listening }"
				data-listen
				>{{ listen }}</span
			>
			<span v-if="card.updated_ms" class="upd">{{
				$t('decisions.card.updatedAt', { time: updatedLabel(card.updated_ms) })
			}}</span>
			<span>{{ $t('decisions.card.position', { position, total }) }}</span>
		</div>
		<h1>{{ card.title }}</h1>
		<div
			v-if="contextHtml"
			class="md"
			@click="onContextClick"
			v-html="contextHtml"
		></div>
		<div v-if="sections.length" class="links">
			<div v-for="section in sections" :key="section.title" class="lg">
				<span class="lh">{{ section.title }}</span>
				<template v-for="entry in section.entries" :key="entry.url">
					<button
						v-if="isHttpUrl(entry.url)"
						type="button"
						class="chip"
						:title="entry.url"
						@click="openLink(entry.url)"
					>
						{{ entry.label }}
					</button>
					<span v-else class="chip">{{ entry.label }}</span>
				</template>
			</div>
		</div>
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
					v-if="fact.link && isHttpUrl(fact.link)"
					type="button"
					class="chip"
					:title="fact.link"
					@click="openLink(fact.link)"
				>
					{{ linkLabel(fact.link) }}
				</button>
				<span v-else-if="fact.link" class="chip">{{
					linkLabel(fact.link)
				}}</span>
			</div>
		</div>

		<div v-if="delivery" class="notice">{{ delivery }}</div>
		<div v-if="unpicked" class="notice unpicked">{{ unpicked }}</div>

		<div v-if="mode === 'final'" class="notice final">
			<div class="what">{{ finalTitle }}</div>
			<div v-if="finalDetail">{{ finalDetail }}</div>
		</div>

		<template v-else>
			<div v-if="card.question" class="q">{{ card.question }}</div>

			<div v-if="mode === 'dismiss-only'" class="notice">
				{{ $t('decisions.card.dismissOnly') }}
				<div style="margin-top: 10px">
					<button
						type="button"
						class="btn"
						@click="$emit('answer', { dismissed: true })"
					>
						{{ $t('decisions.card.dismiss') }} <kbd>X</kbd>
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
						<span v-if="option.recommended" class="star">{{
							$t('decisions.card.recommended')
						}}</span>
						<span v-else class="spacer"></span>
						<button
							type="button"
							class="add"
							:title="$t('decisions.card.addNoteTitle')"
							@click.stop="pick(index, true)"
						>
							{{ $t('decisions.card.addNote') }}
						</button>
					</div>
					<div v-if="noteFor === option.id" class="note">
						<textarea
							ref="note"
							v-model="note"
							data-input="note"
							:placeholder="
								$t('decisions.card.notePlaceholder', { label: option.label })
							"
							@keydown="onNoteKey($event, option.id)"
						></textarea>
						<div class="tf">
							<span>{{ $t('decisions.card.noteSend') }}</span>
							<span>{{ $t('decisions.card.noteCancel') }}</span>
						</div>
					</div>
				</div>
				<div v-if="viaRunner" class="notice">
					{{ $t('decisions.card.viaRunner') }}
				</div>
				<textarea
					ref="free"
					v-model="freeText"
					class="free"
					rows="1"
					data-input="free"
					:placeholder="
						card.options.length
							? $t('decisions.card.freeWithOptions')
							: $t('decisions.card.freeOnly')
					"
					@keydown="onFreeKey"
				></textarea>
				<div class="hints">
					<span v-if="hasRecommended"
						><kbd>Enter</kbd> {{ $t('decisions.card.hints.accept') }}</span
					>
					<span v-if="card.options.length"
						><kbd>1</kbd>–<kbd>{{ card.options.length }}</kbd>
						{{ $t('decisions.card.hints.pick') }}</span
					>
					<span
						><kbd>⇧</kbd>+<kbd>1</kbd>… / <kbd>⇧Enter</kbd>
						{{ $t('decisions.card.hints.pickNote') }}</span
					>
					<span><kbd>D</kbd> {{ $t('decisions.card.hints.defer') }}</span>
					<span><kbd>X</kbd> {{ $t('decisions.card.hints.dismiss') }}</span>
					<span><kbd>C</kbd> {{ $t('decisions.card.hints.chat') }}</span>
					<span
						><kbd>↑</kbd><kbd>↓</kbd>
						{{ $t('decisions.card.hints.cards') }}</span
					>
				</div>
			</template>
		</template>
	</section>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { autosizeTextarea } from '@/utils/commentComposer';
	import { defineComponent, type PropType } from 'vue';
	import {
		cardMode,
		deliveryLabel,
		listenLabel,
		unpickedLabel,
		type CardMode,
	} from '../answer';
	import {
		isHttpUrl,
		kindLabel,
		linkLabel,
		linkSections,
		recommendedOption,
		sourceLabel,
		updatedLabel,
	} from '../format';
	import { isSubmitKey } from '../keys';
	import { clickedHttpLink, renderMarkdown } from '../markdown';
	import type { CardView } from '../types';

	const PICK_DELAY_MS = 180;
	const FREE_MAX_HEIGHT = 240;

	export default defineComponent({
		name: 'CardPane',
		props: {
			card: { type: Object as PropType<CardView>, required: true },
			position: { type: Number, required: true },
			total: { type: Number, required: true },
			freeDraft: { type: String, default: '' },
			noteDraft: { type: String, default: '' },
			noteForDraft: { type: String, default: '' },
		},
		emits: ['answer', 'open-link', 'update:draft'],
		data() {
			const recommended = recommendedOption(this.card);
			return {
				FACT_GLYPH: { ok: '✓', bad: '✗', wait: '◷' },
				selected: (this.noteForDraft || recommended?.id || null) as
					string | null,
				noteFor: (this.noteForDraft || null) as string | null,
				note: this.noteDraft,
				freeText: this.freeDraft,
				pickTimer: 0 as number,
			};
		},
		computed: {
			sections() {
				return linkSections(this.card.links);
			},
			mode(): CardMode {
				return cardMode(this.card);
			},
			delivery(): string | null {
				return deliveryLabel(this.card.delivery);
			},
			listen(): string | null {
				return listenLabel(this.card);
			},
			unpicked(): string | null {
				return unpickedLabel(this.card);
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
				return renderMarkdown(this.card.context ?? '');
			},
			finalTitle(): string {
				const { state, answer } = this.card;
				if (state === 'dismissed') return t('decisions.card.dismissed');
				if (state === 'closed') return t('decisions.card.closed');
				if (answer?.deferred) return t('decisions.card.deferred');
				return t('decisions.card.answered');
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
		watch: {
			freeText(value: string) {
				this.$emit('update:draft', { field: 'free', value });
				void this.$nextTick(() => this.sizeFree());
			},
			noteFor(value: string | null) {
				this.$emit('update:draft', { field: 'noteFor', value: value ?? '' });
			},
			note(value: string) {
				this.$emit('update:draft', { field: 'note', value });
			},
		},
		mounted() {
			this.sizeFree();
		},
		beforeUnmount() {
			window.clearTimeout(this.pickTimer);
		},
		methods: {
			kindLabel,
			sourceLabel,
			linkLabel,
			isHttpUrl,
			updatedLabel,
			openLink(url: string) {
				this.$emit('open-link', url);
			},
			onContextClick(event: MouseEvent) {
				const href = clickedHttpLink(event);
				if (href) this.openLink(href);
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
			sizeFree() {
				autosizeTextarea(
					this.$refs.free as HTMLTextAreaElement | undefined,
					FREE_MAX_HEIGHT,
				);
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
