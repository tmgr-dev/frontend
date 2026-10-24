<template>
	<div
		class="grid grid-cols-2 gap-3 rounded-md border border-border bg-muted/30 p-3 text-xs sm:grid-cols-1"
		data-testid="preset-preview"
	>
		<section>
			<h4 class="mb-1 font-medium tracking-wide text-ink-subtle uppercase">
				Menu
			</h4>
			<ul class="flex flex-col gap-1">
				<li
					v-for="item in nav"
					:key="item.id"
					:data-testid="`preview-nav-${item.id}`"
					:data-state="item.state"
					:class="rowClass(item.state)"
				>
					<span>{{ item.label }}</span>
					<span v-if="badge(item.state)" :class="badgeClass(item.state)">{{
						badge(item.state)
					}}</span>
				</li>
			</ul>
		</section>
		<section>
			<h4 class="mb-1 font-medium tracking-wide text-ink-subtle uppercase">
				Task
			</h4>
			<div
				class="mb-1 flex items-center gap-2 rounded border border-border bg-surface px-2 py-1 text-ink"
				data-testid="preview-card"
				:data-mode="card"
			>
				<span class="min-w-0 flex-1 truncate">Task title</span>
				<span
					v-if="card === 'done'"
					class="flex shrink-0 items-center gap-1"
					data-testid="preview-card-done"
				>
					<span
						class="inline-block h-3 w-3 rounded-sm border border-ink-subtle"
					/>
					Done
				</span>
				<span
					v-else
					class="shrink-0 rounded bg-muted px-1.5 text-2xs text-ink-subtle"
					data-testid="preview-card-status"
					>Status</span
				>
			</div>
			<ul class="flex flex-col gap-1">
				<li
					v-for="item in task"
					:key="item.id"
					:data-testid="`preview-task-${item.id}`"
					:data-state="item.state"
					:class="rowClass(item.state)"
				>
					<span>{{ item.label }}</span>
					<span v-if="badge(item.state)" :class="badgeClass(item.state)">{{
						badge(item.state)
					}}</span>
				</li>
			</ul>
		</section>
	</div>
</template>

<script setup lang="ts">
	import type { PreviewItem, PreviewState } from '@/utils/previewSurfaces';

	defineProps<{
		nav: PreviewItem[];
		task: PreviewItem[];
		card?: 'status' | 'done';
	}>();

	const rowClass = (state: PreviewState) => [
		'flex items-center justify-between gap-2 rounded px-2 py-1 text-ink',
		state === 'off' || state === 'will-hide'
			? 'bg-muted/40 text-ink-subtle line-through opacity-60'
			: 'bg-surface',
	];
	const badge = (state: PreviewState) =>
		({ off: 'off', new: 'new', 'will-hide': 'will hide', on: '' })[state];
	const badgeClass = (state: PreviewState) => [
		'shrink-0 rounded px-1.5 text-2xs no-underline',
		state === 'new'
			? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
			: 'bg-muted text-ink-subtle',
	];
</script>
