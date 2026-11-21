<template>
	<div class="audit">
		<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
		<div v-for="(a, i) in entries" :key="i" class="ar">
			<i class="mono">{{ clock(a.ts_ms) }}</i>
			<div>
				<b class="mono">{{ a.action }}</b>
				<small>{{ a.detail }}</small>
			</div>
			<span :class="{ ow: a.needs_approval }">{{
				a.needs_approval
					? $t('manager.panel.needsApproval')
					: $t('manager.panel.auto')
			}}</span>
		</div>
		<div v-if="!entries.length && !error" class="empty">
			<b>{{ $t('manager.panel.auditEmpty') }}</b>
		</div>
		<p class="hint">{{ $t('manager.panel.auditHint') }}</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { AuditEntry } from '../types';
	import { formatClock } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerAuditTab',
		props: {
			entries: { type: Array as PropType<AuditEntry[]>, required: true },
			error: { type: String, default: '' },
		},
		methods: { clock: formatClock },
	});
</script>
