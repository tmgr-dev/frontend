<template>
	<div
		v-if="error || items.length > 0"
		class="flex flex-col gap-2 border-t border-border pt-3"
		data-testid="deleted-skills"
	>
		<h5 class="text-xs font-semibold">Recently deleted</h5>
		<div
			v-if="error"
			class="flex items-center gap-2 text-xs text-destructive"
			data-testid="deleted-skills-error"
			role="alert"
		>
			Could not load recently deleted skills.
			<Button
				variant="outline"
				size="sm"
				:disabled="loading"
				data-testid="deleted-skills-retry"
				@click="load"
			>
				Retry
			</Button>
		</div>
		<div
			v-for="item in items"
			:key="item.slug"
			class="flex flex-col gap-2 rounded border border-border p-2"
			:data-testid="`deleted-skill-${item.slug}`"
		>
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0">
					<p class="truncate text-sm font-medium">{{ item.title }}</p>
					<p class="truncate text-xs text-muted-foreground">
						{{ item.slug }} ·
						<template v-if="item.files_count"
							>{{ item.files_count }}
							{{ item.files_count === 1 ? 'file' : 'files' }} ·
						</template>
						Deleted {{ relative(item.deleted_at) }} · Removed for good on
						{{ date(item.purge_at) }}
					</p>
				</div>
				<Button
					variant="outline"
					size="sm"
					:aria-expanded="expanded === item.slug"
					:data-testid="`deleted-skill-toggle-${item.slug}`"
					@click="toggle(item.slug)"
				>
					{{ expanded === item.slug ? 'Hide versions' : 'Versions' }}
				</Button>
			</div>
			<div
				v-if="expanded === item.slug"
				class="flex flex-col gap-3 md:flex-row"
			>
				<ul class="flex shrink-0 flex-col gap-1 md:w-72">
					<li v-for="v in item.versions" :key="v.version">
						<button
							type="button"
							:data-testid="`deleted-version-${v.version}`"
							:aria-current="v.version === selected ? 'true' : undefined"
							:class="[
								'w-full rounded border p-2 text-left text-xs',
								v.version === selected
									? 'border-tmgr-blue bg-tmgr-light-blue/20 dark:bg-tmgr-blue/20'
									: 'border-border hover:bg-muted',
							]"
							@click="selectVersion(v.version)"
						>
							<span class="font-semibold">v{{ v.version }}</span>
							· {{ kindLabel(v) }}
							<span class="block text-muted-foreground">
								{{ v.author_name ?? 'Deleted user' }} ·
								{{ relative(v.created_at) }}
							</span>
						</button>
					</li>
				</ul>
				<div
					v-if="selected !== null"
					class="flex min-w-0 flex-1 flex-col gap-2"
				>
					<p
						v-if="previewError"
						class="text-xs text-destructive"
						role="alert"
						data-testid="deleted-preview-error"
					>
						Couldn't load this version. Try again.
					</p>
					<div
						v-else-if="preview"
						class="flex flex-col gap-2"
						data-testid="deleted-preview"
					>
						<p class="text-sm font-medium">{{ preview.title }}</p>
						<p class="text-xs text-muted-foreground">{{ preview.when }}</p>
						<pre
							class="max-h-80 overflow-auto rounded bg-muted p-2 font-mono text-xs whitespace-pre-wrap"
							>{{ preview.body }}</pre>
					</div>
					<div class="flex flex-wrap items-center gap-2">
						<Button
							size="sm"
							:disabled="restoring || !preview"
							data-testid="deleted-restore"
							@click="restore"
						>
							Restore
						</Button>
						<span class="text-xs text-muted-foreground">
							Persona attachments are not restored.
						</span>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import type { DeletedSharedSkillVersionRow } from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { useDeletedSkills } from '@/composable/deletedSkills';
	import { formatRelativeTime } from '@/utils/dailyRoutines/relativeTime';
	import { defineComponent, onMounted, watch } from 'vue';

	const KIND_LABELS: Record<string, string> = {
		baseline: 'Baseline',
		create: 'Created',
		edit: 'Edited',
		restore: 'Restored',
		convert: 'Converted',
	};

	export default defineComponent({
		name: 'DeletedSkills',
		components: { Button },
		props: {
			reloadToken: { type: Number, default: 0 },
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const deleted = useDeletedSkills(() => emit('changed'));

			const kindLabel = (v: DeletedSharedSkillVersionRow) =>
				v.kind === 'restore' && v.restored_from
					? `Restored from v${v.restored_from}`
					: (KIND_LABELS[v.kind] ?? v.kind);

			watch(
				() => props.reloadToken,
				() => deleted.load(),
			);
			onMounted(deleted.load);

			return {
				...deleted,
				kindLabel,
				relative: (iso: string) => formatRelativeTime(iso),
				date: (iso: string) =>
					new Date(iso).toLocaleDateString('en-US', {
						year: 'numeric',
						month: 'short',
						day: 'numeric',
					}),
			};
		},
	});
</script>
