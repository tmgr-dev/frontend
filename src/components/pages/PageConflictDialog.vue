<template>
	<Dialog :open="true" @update:open="onOpen">
		<DialogContent
			class="max-h-[90vh] max-w-4xl overflow-y-auto"
			data-testid="page-conflict-dialog"
		>
			<DialogHeader>
				<DialogTitle>{{ $t('pages.conflict.title') }}</DialogTitle>
				<DialogDescription>
					{{ $t('pages.conflict.description', { version: theirs.version }) }}
				</DialogDescription>
			</DialogHeader>

			<div v-if="comparing" class="grid gap-3 md:grid-cols-2">
				<div>
					<h3 class="mb-1 text-sm font-semibold">
						{{ $t('pages.conflict.theirVersion') }}
					</h3>
					<pre
						class="max-h-72 overflow-auto rounded-md border border-line bg-surface-sunken p-2 text-xs break-words whitespace-pre-wrap"
						data-testid="conflict-theirs"
						>{{ theirs.body }}</pre>
				</div>
				<div>
					<h3 class="mb-1 text-sm font-semibold">
						{{ $t('pages.conflict.yourDraft') }}
					</h3>
					<pre
						class="max-h-72 overflow-auto rounded-md border border-line bg-surface-sunken p-2 text-xs break-words whitespace-pre-wrap"
						data-testid="conflict-mine"
						>{{ draftBody }}</pre>
				</div>
				<div class="md:col-span-2">
					<h3 class="mb-1 text-sm font-semibold">
						{{ $t('pages.conflict.difference') }}
					</h3>
					<PageDiff :before="theirs.body" :after="draftBody" />
				</div>
			</div>
			<p v-else class="text-sm text-ink-muted">
				{{ $t('pages.conflict.hint', { version: theirs.version }) }}
			</p>

			<DialogFooter class="gap-2 sm:gap-2">
				<Button
					variant="outline"
					data-testid="conflict-both"
					@click="$emit('choose', 'both')"
				>
					{{ $t('pages.conflict.showBoth') }}
				</Button>
				<Button
					variant="outline"
					data-testid="conflict-theirs-btn"
					@click="$emit('choose', 'theirs')"
				>
					{{ $t('pages.conflict.takeTheirs') }}
				</Button>
				<Button
					data-testid="conflict-mine-btn"
					@click="$emit('choose', 'mine')"
				>
					{{ $t('pages.conflict.keepMine') }}
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
