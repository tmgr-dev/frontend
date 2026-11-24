<template>
	<div
		:class="['dcard', 'permit', { danger, resolved: !!closedText }]"
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
		<div v-if="error" class="dc-note dc-err" role="alert">{{ error }}</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { permitAnswer } from '../api';
	import { stripFormat } from '../permitCards';
	import type { PermitCardWire, PermitChoice } from '../types';
	import Icon from './Icon.vue';

	export default defineComponent({
		name: 'ManagerPermitCard',
		components: { Icon },
		props: {
			cardId: { type: String, required: true },
			permit: { type: Object as PropType<PermitCardWire>, required: true },
			closed: { type: String, default: undefined },
			answerable: { type: Boolean, default: true },
			live: { type: Boolean, default: false },
			replaced: { type: Boolean, default: false },
		},
		data() {
			return {
				confirming: null as PermitChoice | null,
				busy: false,
				sent: false,
				error: '',
				now: Date.now(),
				timer: 0 as ReturnType<typeof setTimeout> | 0,
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
			canAnswer(): boolean {
				return this.answerable && this.live && this.permit.desktop_ok === true;
			},
			closedText(): string {
				const reason = this.closed && stripFormat(this.closed);
				if (reason === undefined) {
					if (this.replaced) return this.$t('manager.permit.replaced');
					if (this.now > this.permit.exp_ms)
						return this.$t('manager.permit.expired');
					return '';
				}
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
		mounted() {
			const wait = this.permit.exp_ms - Date.now();
			if (wait > 0)
				this.timer = setTimeout(
					() => (this.now = Date.now()),
					Math.min(wait + 50, 2147483647),
				);
		},
		beforeUnmount() {
			if (this.timer) clearTimeout(this.timer);
		},
		methods: {
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
					this.error = this.$t('manager.permit.failed', {
						error: String(e),
					});
					this.confirming = null;
				} finally {
					this.busy = false;
				}
			},
		},
	});
</script>
