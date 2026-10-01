<template>
	<Dialog :open="true" @update:open="onOpen">
		<DialogContent
			class="max-h-[90vh] max-w-4xl overflow-y-auto"
			data-testid="page-conflict-dialog"
		>
			<DialogHeader>
				<DialogTitle>Page changed elsewhere</DialogTitle>
				<DialogDescription>
					Version {{ theirs.version }} appeared while you were editing.
					Choose which to keep.
				</DialogDescription>
			</DialogHeader>

			<div v-if="comparing" class="grid gap-3 md:grid-cols-2">
				<div>
					<h3 class="mb-1 text-sm font-semibold">Their version</h3>
					<pre
						class="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md border border-gray-200 bg-gray-50 p-2 text-xs dark:border-gray-700 dark:bg-gray-800"
						data-testid="conflict-theirs"
						>{{ theirs.body }}</pre
					>
				</div>
				<div>
					<h3 class="mb-1 text-sm font-semibold">Your draft</h3>
					<pre
						class="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md border border-gray-200 bg-gray-50 p-2 text-xs dark:border-gray-700 dark:bg-gray-800"
						data-testid="conflict-mine"
						>{{ draftBody }}</pre
					>
				</div>
				<div class="md:col-span-2">
					<h3 class="mb-1 text-sm font-semibold">Difference</h3>
					<PageDiff :before="theirs.body" :after="draftBody" />
				</div>
			</div>
			<p v-else class="text-sm text-gray-600 dark:text-gray-300">
				Take theirs — load version {{ theirs.version }} and discard your draft.
				Keep mine — save your text over version {{ theirs.version }}.
			</p>

			<DialogFooter class="gap-2 sm:gap-2">
				<Button
					variant="outline"
					data-testid="conflict-both"
					@click="$emit('choose', 'both')"
				>
					Show both
				</Button>
				<Button
					variant="outline"
					data-testid="conflict-theirs-btn"
					@click="$emit('choose', 'theirs')"
				>
					Take theirs
				</Button>
				<Button
					data-testid="conflict-mine-btn"
					@click="$emit('choose', 'mine')"
				>
					Keep mine
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import type { Page } from '@/actions/tmgr/pages';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { defineComponent, type PropType } from 'vue';
	import PageDiff from './PageDiff.vue';

	export default defineComponent({
		name: 'PageConflictDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			PageDiff,
		},
		props: {
			theirs: { type: Object as PropType<Page>, required: true },
			draftBody: { type: String, default: '' },
			comparing: { type: Boolean, default: false },
		},
		emits: ['choose', 'dismiss'],
		setup(_props, { emit }) {
			const onOpen = (open: boolean) => {
				if (!open) emit('dismiss');
			};
			return { onOpen };
		},
	});
</script>
