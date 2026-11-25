<template>
	<div
		:class="['dcard', 'poll', { resolved: !!closedText }]"
		:data-card="poll.card_id"
		@keydown.enter.stop
	>
		<div v-for="(q, qi) in view" :key="qi" class="po-q">
			<div v-if="q.header" class="po-chip">{{ q.header }}</div>
			<div class="po-text">{{ q.question }}</div>
			<ul class="po-opts">
				<li v-for="(o, oi) in q.options" :key="oi">
					<button
						v-if="interactive"
						type="button"
						:class="[
							'po-opt',
							{ on: isPicked(qi, oi), recommended: o.recommended },
						]"
						:aria-pressed="isPicked(qi, oi)"
						:disabled="busy"
						@click="onOption(qi, oi)"
					>
						<span class="po-label">{{ o.label }}</span>
						<span v-if="o.recommended" class="po-badge">{{
							$t('manager.poll.recommended')
						}}</span>
						<span v-if="o.description" class="po-desc">{{
							o.description
						}}</span>
					</button>
					<div v-else class="po-opt static">
						<span class="po-label">{{ o.label }}</span>
						<span v-if="o.recommended" class="po-badge">{{
							$t('manager.poll.recommended')
						}}</span>
						<span v-if="o.description" class="po-desc">{{
							o.description
						}}</span>
					</div>
				</li>
			</ul>
			<div v-if="interactive && !q.multi" class="po-other">
				<input
					type="text"
					class="po-other-input"
					:value="others[qi]"
					:placeholder="$t('manager.poll.otherPlaceholder')"
					:aria-label="$t('manager.poll.other')"
					:disabled="busy"
					@input="onOther(qi, ($event.target as HTMLInputElement).value)"
					@keydown.enter.prevent="onOtherEnter"
				/>
				<button
					v-if="immediate"
					type="button"
					class="sp-btn po-other-send"
					:disabled="busy || !others[qi].trim()"
					@click="submit"
				>
					{{ $t('manager.poll.sendOther') }}
				</button>
			</div>
		</div>
		<div v-if="closedText" class="dc-note po-closed">{{ closedText }}</div>
		<div v-else-if="timedOut" class="dc-note po-timeout">
			{{ $t('manager.poll.noResponse') }}
		</div>
		<div v-else-if="busy" class="dc-note po-sending" role="status">
			<span class="po-spinner" aria-hidden="true"></span
			>{{ $t('manager.poll.sending') }}
		</div>
		<div v-else-if="!interactive" class="dc-note po-terminal">
			{{ $t('manager.poll.terminalOnly')
			}}<template v-if="whyNot">: {{ whyNot }}</template>
		</div>
		<div v-else-if="!immediate" class="sp-row po-actions">
			<button
				type="button"
				class="sp-btn primary po-send"
				:disabled="!complete"
				@click="submit"
			>
				{{ $t('manager.poll.send') }}
			</button>
		</div>
		<div v-if="error" class="dc-note dc-err" role="alert">{{ error }}</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { pollAnswer } from '../api';
	import { stripFormat } from '../permitCards';
	import type { PollClose } from '../pollCards';
	import type { PollPick, PollWire, RoomRef } from '../types';

	const IN_FLIGHT_MS = 10000;

	export default defineComponent({
		name: 'ManagerPollCard',
		props: {
			poll: { type: Object as PropType<PollWire>, required: true },
			closed: { type: Object as PropType<PollClose>, default: undefined },
			room: { type: Object as PropType<RoomRef>, required: true },
		},
		data() {
			const n = this.poll.questions?.length ?? 0;
			return {
				picks: Array.from({ length: n }, () => [] as number[]),
				others: Array.from({ length: n }, () => ''),
				busy: false,
				timedOut: false,
				error: '',
				timer: 0 as ReturnType<typeof setTimeout> | 0,
			};
		},
		computed: {
			view(): {
				header: string;
				question: string;
				multi: boolean;
				options: {
					label: string;
					description: string;
					recommended: boolean;
				}[];
			}[] {
				return (this.poll.questions ?? []).map((q) => ({
					header: stripFormat(q.header),
					question: stripFormat(q.question),
					multi: q.multi_select === true,
					options: q.options.map((o) => ({
						label: stripFormat(o.label),
						description: stripFormat(o.description ?? ''),
						recommended: o.recommended === true,
					})),
				}));
			},
			interactive(): boolean {
				return (
					this.poll.can_answer !== false && !this.closedText && !this.timedOut
				);
			},
			immediate(): boolean {
				return this.view.length === 1 && !this.view[0].multi;
			},
			complete(): boolean {
				return this.view.every(
					(_, i) => this.picks[i].length > 0 || this.others[i].trim() !== '',
				);
			},
			whyNot(): string {
				return this.poll.why_not ? stripFormat(this.poll.why_not) : '';
			},
			closedText(): string {
				const c = this.closed;
				if (!c) return '';
				const reason = stripFormat(c.closed);
				const picked = c.picked ? stripFormat(c.picked) : '';
				const why = c.why ? stripFormat(c.why) : '';
				if (reason === 'applied' || reason === 'answered')
					return picked
						? this.$t('manager.poll.answered', { picked })
						: this.$t('manager.poll.answeredPlain');
				if (reason === 'stale') return this.$t('manager.poll.stale');
				if (reason === 'failed')
					return why
						? this.$t('manager.poll.applyFailed', { why })
						: this.$t('manager.poll.applyFailedPlain');
				if (reason === 'terminal' || reason === 'answered in terminal')
					return this.$t('manager.poll.terminal');
				if (reason === 'cancelled') return this.$t('manager.poll.cancelled');
				if (reason === 'gone') return this.$t('manager.poll.gone');
				return this.$t('manager.poll.closed', { reason });
			},
		},
		watch: {
			closedText(text: string) {
				if (!text) return;
				this.stopTimer();
				this.busy = false;
				this.timedOut = false;
				this.error = '';
			},
		},
		beforeUnmount() {
			this.stopTimer();
		},
		methods: {
			stopTimer() {
				if (this.timer) clearTimeout(this.timer);
				this.timer = 0;
			},
			isPicked(qi: number, oi: number): boolean {
				return this.picks[qi].includes(oi);
			},
			onOption(qi: number, oi: number) {
				if (this.busy) return;
				const multi = this.view[qi].multi;
				if (multi) {
					const cur = this.picks[qi];
					this.picks[qi] = cur.includes(oi)
						? cur.filter((x) => x !== oi)
						: [...cur, oi].sort((a, b) => a - b);
					return;
				}
				this.picks[qi] = [oi];
				this.others[qi] = '';
				if (this.immediate) this.submit();
			},
			onOther(qi: number, value: string) {
				this.others[qi] = value;
				if (value.trim()) this.picks[qi] = [];
			},
			onOtherEnter() {
				if (this.immediate ? this.others[0].trim() : this.complete)
					this.submit();
			},
			buildAnswers(): PollPick[] {
				return this.view.map((_, i) => {
					const text = this.others[i].trim();
					return text
						? { question_idx: i, choices: [], other: text }
						: { question_idx: i, choices: [...this.picks[i]] };
				});
			},
			async submit() {
				if (this.busy || this.timedOut || !this.complete) return;
				this.busy = true;
				this.error = '';
				this.stopTimer();
				this.timer = setTimeout(() => {
					this.timer = 0;
					this.busy = false;
					this.timedOut = true;
				}, IN_FLIGHT_MS);
				try {
					await pollAnswer(this.room, this.poll.card_id, this.buildAnswers());
				} catch (e) {
					this.stopTimer();
					this.busy = false;
					this.error = this.$t('manager.poll.failed', { error: String(e) });
				}
			},
		},
	});
</script>
