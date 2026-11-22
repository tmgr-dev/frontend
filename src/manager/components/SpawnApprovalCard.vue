<template>
	<div
		:class="['dcard', 'spawn', status.tone, { resolved }]"
		:data-card="cardId"
		:data-kind="view.kind"
	>
		<div v-if="resolved" class="sp-resolved">
			<Icon :name="outcomeIcon" />
			<b>{{ $t(`manager.spawn.resolved.${statusText.outcome}`) }}</b>
			<span class="sp-sep">·</span>
			<span class="sp-rtitle">{{ title }}</span>
		</div>
		<div v-else class="dc-title"><Icon name="shield" />{{ title }}</div>
		<div v-if="!resolved && view.chips.length" class="sp-chips" role="group">
			<span
				v-for="chip in view.chips"
				:key="chip.key"
				:class="['sp-chip', chip.tone]"
				><Icon name="alert" />{{ $t(`manager.spawn.chip.${chip.key}`) }}</span
			>
		</div>
		<div v-if="!resolved && view.target" class="sp-target">
			<b class="mono">{{ view.target.label }}</b>
			<span class="sp-machine">{{ view.target.machine }}</span>
			<span :class="['pill', pillClass]"
				><Icon :name="pillIcon" />{{
					$t(`manager.spawn.state.${failed ? 'failed' : statusText.outcome}`)
				}}</span
			>
		</div>
		<div v-if="!resolved && view.structured" class="sp-proposer">
			<Icon name="info" />{{ proposer }}
		</div>
		<dl v-if="!resolved && view.lines.length" class="sp-lines">
			<template v-for="line in view.lines" :key="line.key">
				<dt>{{ $t(`manager.spawn.${line.key}`) }}</dt>
				<dd :class="{ 'sp-warn': line.warn }">
					{{ line.i18n ? $t(line.i18n) : line.value }}
				</dd>
			</template>
		</dl>
		<div class="sp-payload-box">
			<div class="sp-payload-label">{{ $t('manager.spawn.payload') }}</div>
			<pre class="sp-payload">{{ view.payload }}</pre>
		</div>
		<div v-if="failed" class="dc-note dc-err" role="alert">
			{{ $t('manager.spawn.unavailable') }}
		</div>
		<div v-else-if="current" class="dc-note sp-status">
			{{ $t(statusText.key, statusText.params) }}
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { spawnCard } from '../api';
	import { spawnStatusText, spawnView } from '../spawnCard';
	import type { SpawnCardStatus, SpawnCardWire } from '../types';
	import Icon from './Icon.vue';

	const POLL_MS = 2000;
	const TICK_MS = 1000;

	export default defineComponent({
		name: 'ManagerSpawnApprovalCard',
		components: { Icon },
		props: {
			cardId: { type: String, required: true },
			spawn: { type: Object as PropType<SpawnCardWire>, required: true },
		},
		data() {
			return {
				current: null as SpawnCardStatus | null,
				failed: false,
				now: Date.now(),
				poll: 0 as unknown as ReturnType<typeof setInterval>,
				tick: 0 as unknown as ReturnType<typeof setInterval>,
				gone: false,
				approvedSeen: false,
			};
		},
		computed: {
			view() {
				return spawnView(this.spawn);
			},
			statusText() {
				return spawnStatusText(
					this.current ?? { state: 'issued' },
					this.spawn.exp_ms,
					this.now,
					this.approvedSeen,
				);
			},
			title(): string {
				const t = this.view.title;
				return t ? this.$t(t.key, t.params) : this.view.summary;
			},
			proposer(): string {
				const p = this.view.proposer;
				return p?.key === 'session'
					? this.$t('manager.spawn.requestedBySession', {
							session: p.session ?? '',
						})
					: this.$t('manager.spawn.proposedByManager');
			},
			resolved(): boolean {
				return !this.failed && !!this.current && this.statusText.final;
			},
			pillClass(): string {
				return { wait: 'st-waiting', ok: 'st-done', fail: 'st-failed' }[
					this.status.tone
				] as string;
			},
			pillIcon(): string {
				return { wait: 'hand', ok: 'check', fail: 'x' }[
					this.status.tone
				] as string;
			},
			outcomeIcon(): string {
				return this.statusText.outcome === 'approved' ? 'check' : 'x';
			},
			status() {
				return { tone: this.failed ? 'fail' : this.statusText.tone };
			},
		},
		mounted() {
			void this.refresh();
			this.poll = setInterval(() => void this.refresh(), POLL_MS);
			this.tick = setInterval(() => (this.now = Date.now()), TICK_MS);
		},
		beforeUnmount() {
			this.gone = true;
			this.stop();
		},
		methods: {
			stop() {
				clearInterval(this.poll);
				clearInterval(this.tick);
			},
			async refresh() {
				try {
					const status = await spawnCard(this.cardId);
					if (this.gone) return;
					this.current = status;
					if (status.state === 'approved' || status.state === 'claimed') {
						this.approvedSeen = true;
					}
					this.failed = false;
					this.now = Date.now();
					if (this.statusText.final) this.stop();
				} catch {
					if (!this.gone) this.failed = true;
				}
			},
		},
	});
</script>
