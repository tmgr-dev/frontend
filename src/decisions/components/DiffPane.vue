<template>
	<div class="pane">
		<div v-if="!hasDiff" class="placeholder">
			No changes attached to this card.
		</div>
		<div v-else-if="!diff || diff.status === 'loading'" class="placeholder">
			Loading changes…
		</div>
		<div v-else-if="diff.status === 'error'" class="placeholder">
			Could not load changes: {{ diff.error }}
		</div>
		<template v-else-if="diff.groups">
			<div v-if="diff.groups.length === 0" class="placeholder">No changes.</div>
			<section v-for="(group, gi) in panes" :key="group.key" class="dgroup">
				<div v-if="panes.length > 1" class="gh">
					<span class="gl">{{ group.label }}</span>
					<span class="gk">{{ group.kind }}</span>
					<span v-if="group.base || group.head" class="gr">{{
						rangeLabel(group.base, group.head)
					}}</span>
				</div>
				<div v-if="group.view.error" class="placeholder">
					{{ group.view.error }}
				</div>
				<div v-else-if="group.view.files.length === 0" class="placeholder">
					No changes.
				</div>
				<div v-if="group.view.truncated" class="placeholder">
					The diff is too large and was truncated.
				</div>
				<div v-for="(file, fi) in group.files" :key="file.path" class="file">
					<div class="fh">
						<span class="path">{{ file.path }}</span>
						<span class="stat"
							><span class="a">+{{ file.added }}</span>
							<span class="d"> −{{ file.removed }}</span></span
						>
					</div>
					<div class="dv">
						<template v-for="(hunk, hi) in file.hunks" :key="hi">
							<div class="dl hunk">
								<span class="no"></span><span class="no"></span>
								<span class="cb" style="visibility: hidden"></span>
								<span class="code">{{ hunk.header }}</span>
							</div>
							<template v-for="(line, li) in hunk.lines" :key="li">
								<div
									class="dl"
									:class="[line.kind, { hascm: hasThread(file.qpath, line) }]"
								>
									<span class="no">{{ line.old ?? '' }}</span>
									<span class="no">{{ line.new ?? '' }}</span>
									<button
										type="button"
										class="cb"
										title="Comment on this line"
										@click="openThread(file.qpath, line, 'reply')"
									>
										+
									</button>
									<span class="code"
										>{{ PREFIX[line.kind] }}{{ line.text }}</span
									>
								</div>
								<div
									v-if="anchored(gi, fi, hi, li, file.qpath, line)"
									class="thread"
									:data-key="keyOf(file.qpath, line)"
								>
									<div
										v-for="message in threadMessages(file.qpath, line)"
										:key="message.seq"
										class="cm"
										:class="message.author"
									>
										<div class="who">
											<template v-if="message.author === 'owner'">You</template>
											<template v-else><i></i>Agent</template>
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
										v-for="(draft, di) in draftsFor(file.qpath, line)"
										:key="'d' + di"
										class="cm owner"
									>
										<div class="who">You · will be sent with your answer</div>
										{{ draft.comment.text }}
										<div class="acts">
											<button
												type="button"
												@click="$emit('remove-draft', draft.index)"
											>
												Remove
											</button>
										</div>
									</div>
									<div v-if="waitNotice(file.qpath, line)" class="cm typing">
										{{ waitNotice(file.qpath, line) }}
									</div>
									<div v-if="canAct(file.qpath, line)" class="cm">
										<div class="acts">
											<button
												type="button"
												class="pri"
												@click="fix(file.qpath, line, '')"
											>
												Fix
											</button>
											<button
												type="button"
												@click="openThread(file.qpath, line, 'fix')"
											>
												Fix + clarify…
											</button>
											<button type="button" @click="keep(file.qpath, line)">
												Keep
											</button>
											<button
												type="button"
												@click="openThread(file.qpath, line, 'reply')"
											>
												Reply
											</button>
										</div>
									</div>
									<div v-if="resolvedLabel(file.qpath, line)" class="resolved">
										{{ resolvedLabel(file.qpath, line) }}
									</div>
									<template v-if="isComposing(file.qpath, line)">
										<textarea
											:ref="setComposer"
											v-model="texts[keyOf(file.qpath, line)]"
											data-input="thread"
											:placeholder="composerPlaceholder(file.qpath, line)"
											@keydown="onComposerKey($event, file.qpath, line)"
										></textarea>
										<div class="tf">
											<span>{{ composerHint }}</span>
											<span class="btns">
												<button
													v-if="canReply && !final"
													type="button"
													class="pri"
													@click="submit(file.qpath, line, 'send')"
												>
													{{
														modes[keyOf(file.qpath, line)] === 'fix'
															? 'Fix'
															: 'Send'
													}}
												</button>
												<button
													v-if="
														!final && modes[keyOf(file.qpath, line)] !== 'fix'
													"
													type="button"
													@click="submit(file.qpath, line, 'draft')"
												>
													Add to answer
												</button>
												<button type="button" @click="close(file.qpath, line)">
													Close
												</button>
											</span>
										</div>
									</template>
								</div>
							</template>
						</template>
					</div>
				</div>
			</section>
		</template>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import {
		agentWaitNotice,
		awaitsOwner,
		buildAnchors,
		fixMessageText,
		lineNumber,
		qualifyFile,
		remapLegacyThreads,
		sideOf,
		splitMessages,
		threadKey,
		type LineAnchor,
	} from '../diff';
	import { isSubmitKey } from '../keys';
	import { clickedHttpLink, isAgentMessage, renderMarkdown } from '../markdown';
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

	export default defineComponent({
		name: 'DiffPane',
		props: {
			diff: { type: Object as PropType<DiffSlot | null>, default: null },
			hasDiff: { type: Boolean, required: true },
			messages: { type: Array as PropType<CardMessage[]>, required: true },
			drafts: { type: Array as PropType<LineComment[]>, required: true },
			canReply: { type: Boolean, required: true },
			final: { type: Boolean, required: true },
			listening: { type: Boolean, required: true },
			sessionGone: { type: Boolean, required: true },
			now: { type: Number, required: true },
		},
		emits: ['send', 'add-draft', 'remove-draft', 'open-link'],
		data() {
			return {
				PREFIX: { add: '+', del: '-', ctx: ' ' },
				open: {} as Record<string, boolean>,
				modes: {} as Record<string, ComposerMode>,
				texts: {} as Record<string, string>,
				resolved: {} as Record<string, string>,
				composer: null as HTMLTextAreaElement | null,
			};
		},
		computed: {
			panes() {
				const groups = this.diff?.groups ?? [];
				return groups.map((group) => ({
					...group,
					view: group.view,
					files: group.view.files.map((file) => ({
						...file,
						qpath: qualifyFile(group.label, file.path, groups.length),
					})),
				}));
			},
			anchors(): Map<string, LineAnchor>[] {
				const groups = this.diff?.groups ?? [];
				return groups.map((group) =>
					buildAnchors(group.view, groups.length > 1 ? group.label : null),
				);
			},
			threads(): Map<string, CardMessage[]> {
				return remapLegacyThreads(
					splitMessages(this.messages).threads,
					this.anchors,
				);
			},
			composerHint(): string {
				return this.canReply && !this.final
					? 'Enter — send to the agent · Shift+Enter — new line'
					: 'Enter — add to your answer · Shift+Enter — new line';
			},
		},
		methods: {
			isAgent: isAgentMessage,
			render: renderMarkdown,
			onMarkdownClick(event: MouseEvent) {
				const href = clickedHttpLink(event);
				if (href) this.$emit('open-link', href);
			},
			keyOf(file: string, line: DiffLine): string {
				return threadKey(file, lineNumber(line) ?? 0, sideOf(line));
			},
			numberOf(line: DiffLine): number | null {
				return lineNumber(line);
			},
			hasThread(file: string, line: DiffLine): boolean {
				const key = this.keyOf(file, line);
				return (
					this.threads.has(key) ||
					this.drafts.some((d) => threadKey(d.file, d.line, d.side) === key)
				);
			},
			rangeLabel(base?: string, head?: string): string {
				return `${base ?? ''}..${head ?? ''}`;
			},
			anchored(
				gi: number,
				fi: number,
				hi: number,
				li: number,
				file: string,
				line: DiffLine,
			): boolean {
				if (lineNumber(line) === null) return false;
				const key = this.keyOf(file, line);
				const anchor = this.anchors[gi]?.get(key);
				if (
					!anchor ||
					anchor.fileIndex !== fi ||
					anchor.hunkIndex !== hi ||
					anchor.lineIndex !== li
				) {
					return false;
				}
				return this.hasThread(file, line) || Boolean(this.open[key]);
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
				return this.resolved[this.keyOf(file, line)] ?? '';
			},
			composerPlaceholder(file: string, line: DiffLine): string {
				return this.modes[this.keyOf(file, line)] === 'fix'
					? 'How exactly to fix it… (empty — just “Fix”)'
					: 'Comment on this line…';
			},
			setComposer(el: unknown) {
				this.composer = (el as HTMLTextAreaElement | null) ?? this.composer;
			},
			openThread(file: string, line: DiffLine, mode: MessageAction) {
				const key = this.keyOf(file, line);
				this.open[key] = true;
				this.modes[key] = mode;
				delete this.resolved[key];
				void this.$nextTick(() => this.composer?.focus());
			},
			close(file: string, line: DiffLine) {
				const key = this.keyOf(file, line);
				this.open[key] = false;
				this.texts[key] = '';
			},
			keep(file: string, line: DiffLine) {
				this.resolved[this.keyOf(file, line)] = 'Kept as is';
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
				this.resolved[key] = 'Sent to the agent: fix';
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
