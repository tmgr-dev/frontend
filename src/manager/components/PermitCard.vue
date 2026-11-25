<template>
	<div
		:class="['dcard', 'permit', { danger, resolved: !!closedText || expired }]"
		:data-card="cardId"
		@keydown.enter.prevent.stop
	>
		<div class="dc-title">
			<Icon name="shield" />{{
				$t('manager.permit.title', { agent: view.agent, tool: view.tool })
			}}
		</div>
		<pre class="pm-text">{{ view.text }}</pre>
		<div class="pm-session">
			{{ $t('manager.permit.session', { session: view.session }) }}
		</div>
		<dl class="pm-lines">
			<template v-if="permit.path">
				<dt>{{ $t('manager.permit.path') }}</dt>
				<dd>{{ view.path }}</dd>
			</template>
			<template v-if="permit.reason">
				<dt>{{ $t('manager.permit.reason') }}</dt>
				<dd>{{ view.reason }}</dd>
			</template>
		</dl>
		<div v-if="permit.warning" class="pm-warn" role="alert">
			{{ view.warning }}
		</div>
		<div v-for="line in view.danger" :key="line" class="pm-warn">
			{{ line }}
		</div>
		<div v-if="closedText" class="dc-note pm-closed">{{ closedText }}</div>
		<template v-else-if="expired">
			<div class="dc-note pm-closed">{{ $t('manager.permit.expired') }}</div>
			<div v-if="refreshable" class="sp-row pm-actions">
				<button
					v-if="!refreshing"
					type="button"
					class="sp-btn pm-refresh"
					@click="refresh"
				>
					{{ $t('manager.permit.refresh') }}
				</button>
				<span v-else class="dc-note pm-refreshing">{{
					$t('manager.permit.refreshing')
				}}</span>
			</div>
		</template>
		<div v-else-if="sent" class="dc-note pm-sent">
			{{ $t('manager.permit.sent') }}
		</div>
		<template v-else-if="!canAnswer">
			<div class="dc-note pm-phone">{{ $t('manager.permit.phoneOnly') }}</div>
		</template>
		<div v-else-if="confirming" class="sp-row pm-actions">
			<button
				type="button"
				class="sp-btn primary pm-confirm"
				:disabled="busy"
				@click="send(confirming, true)"
			>
				{{
					confirming === 'always'
						? $t('manager.permit.confirmAlways')
						: $t('manager.permit.confirmOnce')
				}}
			</button>
			<button
				type="button"
				class="sp-btn pm-cancel"
				:disabled="busy"
				@click="confirming = null"
			>
				{{ $t('manager.permit.cancel') }}
			</button>
		</div>
		<div v-else class="sp-row pm-actions">
			<button
				type="button"
				class="sp-btn pm-once"
				:disabled="busy"
				@click="onOnce"
			>
				{{ $t('manager.permit.once') }}
			</button>
			<button
				v-if="permit.always_label"
				type="button"
				class="sp-btn pm-always"
				:title="view.always_label"
				:disabled="busy"
				@click="confirming = 'always'"
			>
				{{ $t('manager.permit.always') }}
			</button>
			<button
				type="button"
				class="sp-btn pm-deny"
				:disabled="busy"
				@click="send('deny', false)"
			>
				{{ $t('manager.permit.deny') }}
			</button>
		</div>
		<div v-if="touchId && showActions" class="dc-note pm-touch">
			{{ $t('manager.permit.touchId') }}
		</div>
		<div v-if="error" class="dc-note dc-err" role="alert">{{ error }}</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { permitAnswer, permitRefresh } from '../api';
	import { stripFormat } from '../permitCards';
	import type { PermitCardWire, PermitChoice } from '../types';
	import Icon from './Icon.vue';

	const NOT_SET_UP = 'Set up desktop approvals first';

	export default defineComponent({
		name: 'ManagerPermitCard',
		components: { Icon },
		props: {
			cardId: { type: String, required: true },
			permit: { type: Object as PropType<PermitCardWire>, required: true },
			closed: { type: String, default: undefined },
			answerable: { type: Boolean, default: true },
			live: { type: Boolean, default: false },
			selfFp: { type: String, default: '' },
		},
		data() {
			return {
				confirming: null as PermitChoice | null,
				busy: false,
				sent: false,
				error: '',
				now: Date.now(),
				refreshing: false,
				timer: 0 as ReturnType<typeof setTimeout> | 0,
				refreshTimer: 0 as ReturnType<typeof setTimeout> | 0,
			};
		},
		computed: {
			danger(): boolean {
				return !!this.permit.warning || !!this.permit.danger?.length;
			},
			view(): {
				agent: string;
				tool: string;
				text: string;
				session: string;
				path?: string;
				reason?: string;
				warning?: string;
				danger: string[];
				always_label?: string;
			} {
				const p = this.permit;
				const clean = (v?: string) => (v === undefined ? v : stripFormat(v));
				return {
					agent: stripFormat(p.agent),
					tool: stripFormat(p.tool),
					text: stripFormat(p.text),
					session: stripFormat(p.session),
					path: clean(p.path),
					reason: clean(p.reason),
					warning: clean(p.warning),
					danger: (p.danger ?? []).map(stripFormat),
					always_label: clean(p.always_label),
				};
			},
			stamp(): string {
				return `${this.permit.nonce ?? ''}|${this.permit.exp_ms}`;
			},
			expired(): boolean {
				return this.now > this.permit.exp_ms;
			},
			refreshable(): boolean {
				return this.answerable && this.live;
			},
			touchId(): boolean {
				return (
					!!this.selfFp && !!this.permit.presence_fps?.includes(this.selfFp)
				);
			},
			canAnswer(): boolean {
				return (
					this.answerable &&
					this.live &&
					(this.permit.desktop_ok === true || this.touchId)
				);
			},
			showActions(): boolean {
				return (
					!this.closedText && !this.expired && !this.sent && this.canAnswer
				);
			},
			closedText(): string {
				const reason = this.closed && stripFormat(this.closed);
				if (reason === undefined) return '';
				if (reason.startsWith('answered on '))
					return this.$t('manager.permit.answeredOn', {
						device: reason.slice('answered on '.length),
					});
				if (reason === 'answered in attach')
					return this.$t('manager.permit.answeredInAttach');
				if (reason === 'gone') return this.$t('manager.permit.gone');
				if (reason === 'expired') return this.$t('manager.permit.expired');
				return this.$t('manager.permit.closed', { reason });
			},
		},
		watch: {
			stamp() {
				this.sent = false;
				this.confirming = null;
				this.error = '';
				this.stopRefreshing();
				this.arm();
			},
		},
		mounted() {
			this.arm();
		},
		beforeUnmount() {
			if (this.timer) clearTimeout(this.timer);
			if (this.refreshTimer) clearTimeout(this.refreshTimer);
		},
		methods: {
			arm() {
				if (this.timer) clearTimeout(this.timer);
				this.now = Date.now();
				const wait = this.permit.exp_ms - this.now;
				if (wait > 0)
					this.timer = setTimeout(
						() => (this.now = Date.now()),
						Math.min(wait + 50, 2147483647),
					);
			},
			stopRefreshing() {
				if (this.refreshTimer) clearTimeout(this.refreshTimer);
				this.refreshTimer = 0;
				this.refreshing = false;
			},
			async refresh() {
				if (this.refreshing) return;
				this.refreshing = true;
				this.error = '';
				try {
					await permitRefresh(this.cardId);
					this.refreshTimer = setTimeout(() => this.stopRefreshing(), 8000);
				} catch (e) {
					this.error = this.$t('manager.permit.failed', { error: String(e) });
					this.stopRefreshing();
				}
			},
			onOnce() {
				if (this.danger) this.confirming = 'once';
				else this.send('once', false);
			},
			async send(choice: PermitChoice, confirmed: boolean) {
				if (this.busy) return;
				this.busy = true;
				this.error = '';
				try {
					await permitAnswer(this.cardId, choice, confirmed);
					this.sent = true;
					this.confirming = null;
				} catch (e) {
					this.error =
						String(e) === NOT_SET_UP
							? this.$t('manager.permit.setupTouchId')
							: this.$t('manager.permit.failed', { error: String(e) });
					this.confirming = null;
				} finally {
					this.busy = false;
				}
			},
		},
	});
</script>
