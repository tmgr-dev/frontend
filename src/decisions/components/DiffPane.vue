<template>
	<div ref="scroller" class="pane">
		<div v-if="!hasDiff" class="placeholder">
			{{ $t('decisions.diff.noneAttached') }}
		</div>
		<div v-else-if="!diff || diff.status === 'loading'" class="placeholder">
			{{ $t('decisions.diff.loading') }}
		</div>
		<div v-else-if="diff.status === 'error'" class="placeholder">
			{{ $t('decisions.diff.loadFailed', { error: diff.error }) }}
		</div>
		<template v-else-if="diff.groups">
			<div v-if="diff.groups.length === 0" class="placeholder">
				{{ $t('decisions.diff.none') }}
			</div>
			<div
				v-else
				class="vlist"
				:style="{
					height: totalSize + 'px',
					minWidth: 'calc(' + built.maxChars + 'ch + 100px)',
				}"
			>
				<div
					v-for="item in items"
					:key="String(item.key)"
					:ref="measure"
					:data-index="item.index"
					class="vr"
					:class="rowClass(rows[item.index])"
					:style="{ transform: 'translateY(' + item.start + 'px)' }"
				>
					<template v-for="row in [rows[item.index]]" :key="row.id">
						<div v-if="row.kind === 'group'" class="gh">
							<span class="gl">{{ row.group.label }}</span>
							<span class="gk">{{ groupKind(row.group.kind) }}</span>
							<span v-if="row.group.base || row.group.head" class="gr">{{
								rangeLabel(row.group.base, row.group.head)
							}}</span>
						</div>
						<div v-else-if="row.kind === 'note'" class="placeholder">
							<template v-if="row.note === 'error'">{{ row.text }}</template>
							<template v-else-if="row.note === 'none'">{{
								$t('decisions.diff.none')
							}}</template>
							<template v-else>{{ $t('decisions.diff.truncated') }}</template>
						</div>
						<div v-else-if="row.kind === 'file'" class="vb top fh">
							<span class="path">{{ row.file.path }}</span>
							<span class="stat"
								><span class="a">+{{ row.file.added }}</span>
								<span class="d"> −{{ row.file.removed }}</span></span
							>
						</div>
						<div v-else-if="row.kind === 'hunk'" class="vb dl hunk">
							<span class="no"></span><span class="no"></span>
							<span class="cb" style="visibility: hidden"></span>
							<span class="code">{{ row.header }}</span>
						</div>
						<div
							v-else-if="row.kind === 'line'"
							class="vb dl"
							:class="[row.line.kind, { hascm: hasKey(row.key) }]"
						>
							<span class="no">{{ row.line.old ?? '' }}</span>
							<span class="no">{{ row.line.new ?? '' }}</span>
							<button
								type="button"
								class="cb"
								:title="$t('decisions.diff.commentOnLine')"
								@click="openThread(row.qpath, row.line, 'reply')"
							>
								+
							</button>
							<span class="code"
								>{{ PREFIX[row.line.kind] }}{{ row.line.text }}</span
							>
						</div>
						<div v-else class="vb tr">
							<div class="thread" :data-key="row.key">
								<div
									v-for="message in threadMessages(row.qpath, row.line)"
									:key="message.seq"
									class="cm"
									:class="message.author"
								>
									<div class="who">
										<template v-if="message.author === 'owner'">{{
											$t('decisions.diff.you')
										}}</template>
										<template v-else
											><i></i>{{ $t('decisions.diff.agent') }}</template
										>
										<CopyButton v-if="isAgent(message)" :text="message.text" />
									</div>
									<div
										v-if="isAgent(message)"
										class="md"
										@click="onMarkdownClick"
										v-html="render(message.text)"
									></div>
									<template v-else>{{ message.text }}</template>
								</div>
								<div
									v-for="(draft, di) in draftsFor(row.qpath, row.line)"
									:key="'d' + di"
									class="cm owner"
								>
									<div class="who">{{ $t('decisions.diff.draftLabel') }}</div>
									{{ draft.comment.text }}
									<div class="acts">
										<button
											type="button"
											@click="$emit('remove-draft', draft.index)"
										>
											{{ $t('decisions.diff.remove') }}
										</button>
									</div>
								</div>
								<div v-if="waitNotice(row.qpath, row.line)" class="cm typing">
									{{ waitNotice(row.qpath, row.line) }}
								</div>
								<div v-if="canAct(row.qpath, row.line)" class="cm">
									<div class="acts">
										<button
											type="button"
											class="pri"
											@click="fix(row.qpath, row.line, '')"
										>
											{{ $t('decisions.diff.fix') }}
										</button>
										<button
											type="button"
											@click="openThread(row.qpath, row.line, 'fix')"
										>
											{{ $t('decisions.diff.fixClarify') }}
										</button>
										<button type="button" @click="keep(row.qpath, row.line)">
											{{ $t('decisions.diff.keep') }}
										</button>
										<button
											type="button"
											@click="openThread(row.qpath, row.line, 'reply')"
										>
											{{ $t('decisions.diff.reply') }}
										</button>
									</div>
								</div>
								<div v-if="resolvedLabel(row.qpath, row.line)" class="resolved">
									{{ resolvedLabel(row.qpath, row.line) }}
								</div>
								<template v-if="isComposing(row.qpath, row.line)">
									<textarea
										:ref="focusComposer"
										v-model="texts[row.key]"
										data-input="thread"
										:placeholder="composerPlaceholder(row.qpath, row.line)"
										@keydown="onComposerKey($event, row.qpath, row.line)"
									></textarea>
									<div class="tf">
										<span>{{ composerHint }}</span>
										<span class="btns">
											<button
												v-if="canReply && !final"
												type="button"
												class="pri"
												@click="submit(row.qpath, row.line, 'send')"
											>
												{{
													modes[row.key] === 'fix'
														? $t('decisions.diff.fix')
														: $t('decisions.diff.send')
												}}
											</button>
											<button
												v-if="!final && modes[row.key] !== 'fix'"
												type="button"
												@click="submit(row.qpath, row.line, 'draft')"
											>
												{{ $t('decisions.diff.addToAnswer') }}
											</button>
											<button type="button" @click="close(row.qpath, row.line)">
												{{ $t('decisions.diff.close') }}
											</button>
										</span>
									</div>
								</template>
							</div>
						</div>
					</template>
				</div>
			</div>
		</template>
	</div>
</template>

<script lang="ts">
	import { t, te } from '@/i18n';
	import { defaultRangeExtractor, useVirtualizer } from '@tanstack/vue-virtual';
	import {
		computed,
		defineComponent,
		shallowRef,
		toRaw,
		type ComponentPublicInstance,
		type PropType,
	} from 'vue';
	import {
		agentWaitNotice,
		awaitsOwner,
		buildAnchors,
		fixMessageText,
		lineNumber,
		remapLegacyThreads,
		sideOf,
		splitMessages,
		threadKey,
		type LineAnchor,
	} from '../diff';
	import {
		buildDiffRows,
		keySignature,
		withPinnedIndexes,
		type DiffRow,
		type DiffRows,
	} from '../diffRows';
	import { isSubmitKey } from '../keys';
	import { copyClick, renderWithCopy } from '../copy';
	import { clickedHttpLink, isAgentMessage } from '../markdown';
	import CopyButton from './CopyButton.vue';
	import type {
		CardMessage,
		DiffGroup,
		DiffLine,
		LineComment,
		MessageAction,
		OutgoingMessage,
	} from '../types';

	interface DiffSlot {
		status: 'loading' | 'ready' | 'error';
		groups?: DiffGroup[];
		error?: string;
	}

	type ComposerMode = MessageAction | '';

	const LINE_HEIGHT = 20;

	const estimate = (row: DiffRow | undefined): number => {
		if (!row) return LINE_HEIGHT;
		if (row.kind === 'thread') return 120;
		if (row.kind === 'file') return 44;
		if (row.kind === 'group' || row.kind === 'note') return 40;
		return LINE_HEIGHT;
	};

	export default defineComponent({
		name: 'DiffPane',
		components: { CopyButton },
		props: {
			diff: { type: Object as PropType<DiffSlot | null>, default: null },
			hasDiff: { type: Boolean, required: true },
			messages: { type: Array as PropType<CardMessage[]>, required: true },
			drafts: { type: Array as PropType<LineComment[]>, required: true },
			canReply: { type: Boolean, required: true },
			final: { type: Boolean, required: true },
			listening: { type: Boolean, required: true },
			queued: { type: Boolean, default: false },
			typed: { type: Boolean, default: false },
			sessionGone: { type: Boolean, required: true },
			now: { type: Number, required: true },
		},
		emits: ['send', 'add-draft', 'remove-draft', 'open-link'],
		setup() {
			const scroller = shallowRef<HTMLElement | null>(null);
			const rows = shallowRef<DiffRow[]>([]);
			const pinned = shallowRef<number[]>([]);
			const virtualizer = useVirtualizer(
				computed(() => ({
					count: rows.value.length,
					getScrollElement: () => scroller.value,
					estimateSize: (index: number) => estimate(rows.value[index]),
					getItemKey: (index: number) => rows.value[index]?.id ?? index,
					overscan: 30,
					rangeExtractor: (
						range: Parameters<typeof defaultRangeExtractor>[0],
					) =>
						withPinnedIndexes(
							defaultRangeExtractor(range),
							pinned.value,
							rows.value.length,
						),
				})),
			);
			const measure = (el: Element | ComponentPublicInstance | null) =>
				virtualizer.value.measureElement(el as Element | null);
			return { scroller, rows, pinned, virtualizer, measure };
		},
		data() {
			return {
				PREFIX: { add: '+', del: '-', ctx: ' ' },
				open: {} as Record<string, boolean>,
				modes: {} as Record<string, ComposerMode>,
				texts: {} as Record<string, string>,
				resolved: {} as Record<string, 'kept' | 'fixSent'>,
				focusKey: null as string | null,
			};
		},
		computed: {
			anchors(): Map<string, LineAnchor>[] {
				const groups = this.diff?.groups ?? [];
				return groups.map((group) =>
					buildAnchors(
						toRaw(group).view,
						groups.length > 1 ? group.label : null,
					),
				);
			},
			threads(): Map<string, CardMessage[]> {
				return remapLegacyThreads(
					splitMessages(this.messages).threads,
					this.anchors,
				);
			},
			commentedKeys(): Set<string> {
				const keys = new Set<string>(this.threads.keys());
				for (const d of this.drafts)
					keys.add(threadKey(d.file, d.line, d.side));
				return keys;
			},
			activeKeys(): Set<string> {
				const keys = new Set<string>(this.commentedKeys);
				for (const key in this.open) if (this.open[key]) keys.add(key);
				return keys;
			},
			activeSignature(): string {
				return keySignature(this.activeKeys);
			},
			built(): DiffRows {
				const groups = toRaw(this.diff?.groups ?? []);
				const signature = this.activeSignature;
				return buildDiffRows(
					groups,
					new Set(signature ? signature.split('\0') : []),
				);
			},
			items() {
				return this.virtualizer
					.getVirtualItems()
					.filter((item) => item.index < this.rows.length);
			},
			totalSize(): number {
				return this.virtualizer.getTotalSize();
			},
			composerHint(): string {
				return this.canReply && !this.final
					? t('decisions.diff.hintSend')
					: t('decisions.diff.hintDraft');
			},
		},
		watch: {
			built: {
				immediate: true,
				handler(built: DiffRows) {
					this.rows = built.rows;
					this.updatePinned();
				},
			},
			open: {
				deep: true,
				handler() {
					this.updatePinned();
				},
			},
		},
		methods: {
			isAgent: isAgentMessage,
			render: renderWithCopy,
			onMarkdownClick(event: MouseEvent) {
				if (copyClick(event)) return;
				const href = clickedHttpLink(event);
				if (href) this.$emit('open-link', href);
			},
			keyOf(file: string, line: DiffLine): string {
				return threadKey(file, lineNumber(line) ?? 0, sideOf(line));
			},
			numberOf(line: DiffLine): number | null {
				return lineNumber(line);
			},
			hasKey(key: string): boolean {
				return this.commentedKeys.has(key);
			},
			rowClass(row: DiffRow | undefined): Record<string, boolean> {
				return {
					ftop: row?.kind === 'file',
					fbot: row !== undefined && 'bottom' in row && row.bottom,
				};
			},
			updatePinned() {
				const pinned: number[] = [];
				for (const key in this.open) {
					const index = this.open[key]
						? this.built.threadIndex.get(key)
						: undefined;
					if (index !== undefined) pinned.push(index);
				}
				this.pinned = pinned;
			},
			groupKind(kind: string): string {
				const key = `decisions.diff.groupKind.${kind}`;
				return te(key) ? t(key) : kind;
			},
			rangeLabel(base?: string, head?: string): string {
				return `${base ?? ''}..${head ?? ''}`;
			},
			threadMessages(file: string, line: DiffLine): CardMessage[] {
				return this.threads.get(this.keyOf(file, line)) ?? [];
			},
			draftsFor(file: string, line: DiffLine) {
				const key = this.keyOf(file, line);
				return this.drafts
					.map((comment, index) => ({ comment, index }))
					.filter(
						(d) =>
							threadKey(d.comment.file, d.comment.line, d.comment.side) === key,
					);
			},
			waitNotice(file: string, line: DiffLine): string | null {
				if (this.resolved[this.keyOf(file, line)]) return null;
				return agentWaitNotice(this.threadMessages(file, line), {
					listening: this.listening,
					queued: this.queued,
					typed: this.typed,
					sessionGone: this.sessionGone,
					canReply: this.canReply,
					final: this.final,
					now: this.now,
				});
			},
			canAct(file: string, line: DiffLine): boolean {
				const key = this.keyOf(file, line);
				return (
					this.canReply &&
					!this.final &&
					!this.resolved[key] &&
					!this.open[key] &&
					awaitsOwner(this.threadMessages(file, line))
				);
			},
			isComposing(file: string, line: DiffLine): boolean {
				return Boolean(this.open[this.keyOf(file, line)]);
			},
			resolvedLabel(file: string, line: DiffLine): string {
				const state = this.resolved[this.keyOf(file, line)];
				if (state === 'kept') return t('decisions.diff.keptAsIs');
				return state === 'fixSent' ? t('decisions.diff.sentFix') : '';
			},
			composerPlaceholder(file: string, line: DiffLine): string {
				return this.modes[this.keyOf(file, line)] === 'fix'
					? t('decisions.diff.fixPlaceholder')
					: t('decisions.diff.commentPlaceholder');
			},
			focusComposer(el: unknown) {
				const key = this.focusKey;
				if (key === null || !(el instanceof HTMLTextAreaElement)) return;
				if (!el.isConnected) return;
				if (el.closest<HTMLElement>('.thread')?.dataset.key !== key) return;
				this.focusKey = null;
				el.focus();
			},
			openThread(file: string, line: DiffLine, mode: MessageAction) {
				const key = this.keyOf(file, line);
				this.open[key] = true;
				this.modes[key] = mode;
				delete this.resolved[key];
				this.focusKey = key;
				void this.$nextTick(() => {
					const threads =
						this.scroller?.querySelectorAll<HTMLElement>('.thread');
					for (const thread of threads ?? []) {
						if (thread.dataset.key === key) {
							this.focusComposer(thread.querySelector('textarea'));
						}
					}
				});
			},
			close(file: string, line: DiffLine) {
				const key = this.keyOf(file, line);
				this.open[key] = false;
				this.texts[key] = '';
			},
			keep(file: string, line: DiffLine) {
				this.resolved[this.keyOf(file, line)] = 'kept';
			},
			fix(file: string, line: DiffLine, extra: string) {
				const number = lineNumber(line);
				if (number === null) return;
				const message: OutgoingMessage = {
					text: fixMessageText(extra),
					file,
					line: number,
					side: sideOf(line),
					action: 'fix',
				};
				this.$emit('send', message);
				const key = this.keyOf(file, line);
				this.resolved[key] = 'fixSent';
				this.open[key] = false;
				this.texts[key] = '';
			},
			submit(file: string, line: DiffLine, how: 'send' | 'draft') {
				const key = this.keyOf(file, line);
				const text = (this.texts[key] ?? '').trim();
				const number = lineNumber(line);
				if (number === null) return;
				if (this.modes[key] === 'fix') {
					this.fix(file, line, text);
					return;
				}
				if (!text) return;
				if (how === 'send' && this.canReply && !this.final) {
					this.$emit('send', {
						text,
						file,
						line: number,
						side: sideOf(line),
						action: 'reply',
					} as OutgoingMessage);
				} else {
					this.$emit('add-draft', {
						file,
						line: number,
						side: sideOf(line),
						text,
					} as LineComment);
				}
				this.texts[key] = '';
				this.open[key] = false;
			},
			onComposerKey(event: KeyboardEvent, file: string, line: DiffLine) {
				const key = this.keyOf(file, line);
				if (event.key === 'Escape') {
					this.open[key] = false;
					return;
				}
				if (isSubmitKey(event)) {
					event.preventDefault();
					this.submit(
						file,
						line,
						this.canReply && !this.final ? 'send' : 'draft',
					);
				}
			},
		},
	});
</script>
