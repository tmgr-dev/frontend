<template>
	<div
		class="flex flex-col gap-3 rounded border border-border p-3"
		data-testid="shared-skill-history"
	>
		<div class="flex items-center justify-between gap-2">
			<h5 class="truncate text-xs font-semibold">History: {{ title }}</h5>
			<Button variant="outline" size="sm" @click="$emit('close')">
				Close
			</Button>
		</div>
		<p
			v-if="loading && versions.length === 0"
			class="text-xs text-muted-foreground"
		>
			Loading history…
		</p>
		<div v-else class="flex flex-col gap-3 md:flex-row">
			<ul class="flex shrink-0 flex-col gap-1 md:w-72">
				<li v-for="v in versions" :key="v.version">
					<button
						type="button"
						:data-testid="`history-version-${v.version}`"
						:class="[
							'w-full rounded border p-2 text-left text-xs',
							v.version === selected
								? 'border-tmgr-blue bg-tmgr-light-blue/20 dark:bg-tmgr-blue/20'
								: 'border-border hover:bg-muted',
						]"
						@click="select(v.version)"
					>
						<span class="font-semibold">v{{ v.version }}</span>
						· {{ kindLabel(v) }}
						<span
							v-if="v.current"
							class="ml-1 rounded bg-green-100 px-1.5 py-0.5 text-2xs font-semibold text-green-800 dark:bg-green-900/30 dark:text-green-300"
						>
							Current
						</span>
						<span class="block text-muted-foreground">
							{{ v.author ? v.author.name : 'Deleted user' }} ·
							{{ relative(v.created_at) }}
						</span>
						<span
							v-if="v.pinned_personas && v.pinned_personas.length"
							class="block text-muted-foreground"
						>
							Pinned by: {{ v.pinned_personas.map((p) => p.name).join(', ') }}
						</span>
					</button>
				</li>
			</ul>
			<div v-if="selected !== null" class="flex min-w-0 flex-1 flex-col gap-2">
				<div class="flex flex-wrap items-center justify-between gap-2">
					<label class="flex items-center gap-2 text-xs">
						<input
							type="checkbox"
							:checked="compareWithCurrent"
							@change="onCompareChange"
						/>
						Compare with current
					</label>
					<Button
						v-if="selected !== currentVersion"
						size="sm"
						:disabled="restoring"
						@click="confirmRestore"
					>
						Restore this version
					</Button>
				</div>
				<ul
					v-if="metaChanges.length"
					class="flex flex-col gap-1 rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-900 dark:border-amber-700 dark:bg-amber-900/20 dark:text-amber-200"
					data-testid="history-meta-changes"
				>
					<li v-for="change in metaChanges" :key="change">{{ change }}</li>
				</ul>
				<p
					v-if="error"
					class="text-xs text-destructive"
					data-testid="history-error"
					role="alert"
				>
					Couldn't load this version. Try again.
				</p>
				<pre
					v-else
					class="max-h-80 overflow-auto rounded bg-muted p-2 font-mono text-xs"
					data-testid="history-diff"
				><span
					v-for="(line, i) in diff"
					:key="i"
					:class="{
						'block bg-green-100 text-green-900 dark:bg-green-900/30 dark:text-green-200':
							line.type === 'add',
						'block bg-red-100 text-red-900 dark:bg-red-900/30 dark:text-red-200':
							line.type === 'del',
						block: line.type === 'same',
					}"
				>{{ marker(line.type) }}{{ line.text }}</span></pre>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import type { SharedSkillVersionRow } from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { useSharedSkillHistory } from '@/composable/sharedSkillVersions';
	import { formatRelativeTime } from '@/utils/dailyRoutines/relativeTime';
	import { computed, defineComponent, onMounted } from 'vue';

	const KIND_LABELS: Record<string, string> = {
		baseline: 'Baseline',
		create: 'Created',
		edit: 'Edited',
		restore: 'Restored',
		convert: 'Converted',
	};

	export default defineComponent({
		name: 'SharedSkillHistory',
		components: { Button },
		props: {
			slug: { type: String, required: true },
			title: { type: String, required: true },
		},
		emits: ['close', 'changed'],
		setup(props, { emit }) {
			const history = useSharedSkillHistory(props.slug, () => emit('changed'));

			const currentVersion = computed(
				() => history.versions.value.find((v) => v.current)?.version ?? null,
			);

			const kindLabel = (v: SharedSkillVersionRow) =>
				v.kind === 'restore' && v.restored_from
					? `Restored from v${v.restored_from}`
					: (KIND_LABELS[v.kind] ?? v.kind);

			const confirmRestore = () => {
				const version = history.selected.value;
				const current = currentVersion.value;
				if (version === null || current === null) return;
				if (
					!window.confirm(
						`Restore version ${version}? This creates version ${current + 1} with its content.`,
					)
				)
					return;
				history.restore(version);
			};

			const onCompareChange = (event: Event) =>
				history.setCompareWithCurrent(
					(event.target as HTMLInputElement).checked,
				);

			const marker = (type: string) =>
				type === 'add' ? '+ ' : type === 'del' ? '- ' : '  ';

			onMounted(history.load);

			return {
				...history,
				currentVersion,
				kindLabel,
				confirmRestore,
				marker,
				onCompareChange,
				relative: (iso: string) => formatRelativeTime(iso),
			};
		},
	});
</script>
