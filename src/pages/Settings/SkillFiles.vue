<template>
	<div
		class="flex flex-col gap-2 border-t border-border pt-3"
		data-testid="skill-files"
	>
		<h5 class="text-xs font-semibold">Files · {{ slug }}</h5>
		<p v-if="loading" class="text-xs text-muted-foreground">Loading files…</p>
		<ul v-else class="flex flex-col gap-1">
			<li
				v-for="file in files"
				:key="file.path"
				class="flex items-center justify-between gap-3 rounded border border-border p-2 text-xs"
				data-testid="skill-file-row"
			>
				<button
					type="button"
					class="min-w-0 truncate text-left font-mono hover:underline"
					data-testid="skill-file-view"
					@click="view(file.path)"
				>
					{{ file.path }}
				</button>
				<span class="flex shrink-0 items-center gap-2">
					<span class="text-muted-foreground">{{
						formatBytes(file.size)
					}}</span>
					<Button
						variant="outline"
						size="sm"
						:aria-label="`Delete ${file.path}`"
						data-testid="skill-file-delete"
						@click="remove(file.path)"
					>
						Delete
					</Button>
				</span>
			</li>
		</ul>
		<div v-if="openPath" class="flex flex-col gap-1">
			<p class="font-mono text-xs text-muted-foreground">{{ openPath }}</p>
			<pre
				class="max-h-96 overflow-auto rounded border border-border bg-muted p-2 text-xs whitespace-pre-wrap"
				data-testid="skill-file-content"
				>{{ contentLoading ? 'Loading…' : content }}</pre>
		</div>
	</div>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { useSkillFiles } from '@/composable/skillFiles';
	import { formatBytes } from '@/utils/aiAttachments';
	import { defineComponent, onMounted } from 'vue';

	export default defineComponent({
		name: 'SkillFiles',
		components: { Button },
		props: {
			slug: { type: String, required: true },
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const state = useSkillFiles(
				() => props.slug,
				() => emit('changed'),
			);
			onMounted(state.load);
			return { ...state, formatBytes };
		},
	});
</script>
