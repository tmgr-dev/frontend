<template>
	<Dialog
		:open="!!ctx.renaming.value"
		@update:open="(open) => !open && (ctx.renaming.value = null)"
	>
		<DialogContent class="max-w-sm">
			<DialogHeader>
				<DialogTitle>Rename page</DialogTitle>
			</DialogHeader>
			<form class="flex flex-col gap-3" @submit.prevent="ctx.confirmRename">
				<input
					v-model="ctx.renameTitle.value"
					data-selectable
					maxlength="255"
					class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-hidden dark:border-input dark:bg-background"
					placeholder="Title"
				/>
				<DialogFooter>
					<button
						type="submit"
						class="h-9 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
						:disabled="!ctx.renameTitle.value.trim()"
					>
						Save
					</button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>

	<AlertDialog
		:open="!!ctx.deleting.value"
		@update:open="(open) => !open && (ctx.deleting.value = null)"
	>
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>Archive page</AlertDialogTitle>
				<AlertDialogDescription class="[overflow-wrap:anywhere]">
					"{{ deletingTitle }}" and all its subpages will be moved to the
					<router-link
						:to="`/${ctx.workspaceCode.value}/pages/_/archive`"
						class="font-medium text-foreground underline underline-offset-4 hover:text-primary dark:text-foreground dark:hover:text-primary"
						@click="ctx.deleting.value = null"
						>archive</router-link
					>. You can restore them from there.
				</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel>Cancel</AlertDialogCancel>
				<Button variant="destructive" @click="ctx.confirmDelete">
					Archive
				</Button>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
</template>

<script lang="ts">
	import {
		AlertDialog,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { defineComponent, inject, ref, watch } from 'vue';
	import { PAGES_TREE_KEY, type PagesTreeContext } from './context';

	export default defineComponent({
		name: 'PagesTreeDialogs',
		components: {
			AlertDialog,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		setup() {
			const ctx = inject(PAGES_TREE_KEY) as PagesTreeContext;
			const deletingTitle = ref('');
			watch(
				() => ctx.deleting.value,
				(page) => page && (deletingTitle.value = page.title),
				{ immediate: true },
			);
			return { ctx, deletingTitle };
		},
	});
</script>
