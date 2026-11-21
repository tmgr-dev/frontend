<template>
	<div :class="['dcard', 'spawn', status.tone]" :data-card="cardId">
		<div class="dc-title">
			<Icon name="hand" />{{
				view.profile
					? $t('manager.spawn.title', { profile: view.profile })
					: view.summary
			}}
		</div>
		<dl v-if="view.lines.length" class="sp-lines">
			<template v-for="line in view.lines" :key="line.key">
				<dt>{{ $t(`manager.spawn.${line.key}`) }}</dt>
				<dd :class="{ 'sp-warn': line.warn }">{{ line.value }}</dd>
			</template>
		</dl>
		<div v-if="view.bypass" class="dc-note sp-warn" role="alert">
			{{ $t('manager.spawn.bypass') }}
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
				);
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
