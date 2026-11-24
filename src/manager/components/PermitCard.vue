<template>
	<div
		:class="['dcard', 'permit', { danger, resolved: !!closedText }]"
		:data-card="cardId"
		@keydown.enter.prevent.stop
	>
		<div class="dc-title">
			<Icon name="shield" />{{
				$t('manager.permit.title', { agent: permit.agent, tool: permit.tool })
			}}
		</div>
		<pre class="pm-text">{{ permit.text }}</pre>
		<dl class="pm-lines">
			<dt>{{ $t('manager.permit.session') }}</dt>
			<dd>{{ permit.session }}</dd>
			<template v-if="permit.path">
				<dt>{{ $t('manager.permit.path') }}</dt>
				<dd>{{ permit.path }}</dd>
			</template>
			<template v-if="permit.reason">
				<dt>{{ $t('manager.permit.reason') }}</dt>
				<dd>{{ permit.reason }}</dd>
			</template>
		</dl>
		<div v-if="permit.warning" class="pm-warn" role="alert">
			{{ permit.warning }}
		</div>
		<div v-for="line in permit.danger ?? []" :key="line" class="pm-warn">
			{{ line }}
		</div>
		<div v-if="closedText" class="dc-note pm-closed">{{ closedText }}</div>
		<template v-else-if="!canAnswer">
			<div class="dc-note pm-phone">{{ $t('manager.permit.phoneOnly') }}</div>
		</template>
		<div v-else-if="sent" class="dc-note pm-sent">
			{{ $t('manager.permit.sent') }}
		</div>
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
				:title="permit.always_label"
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
		},
		data() {
			return {
				confirming: null as PermitChoice | null,
				busy: false,
				sent: false,
				error: '',
			};
		},
		computed: {
			danger(): boolean {
				return !!this.permit.warning || !!this.permit.danger?.length;
			},
			canAnswer(): boolean {
				return this.answerable && this.permit.desktop_ok === true;
			},
			closedText(): string {
				const reason = this.closed;
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
