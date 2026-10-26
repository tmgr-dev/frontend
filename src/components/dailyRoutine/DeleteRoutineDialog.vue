<template>
	<AlertDialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<AlertDialogPortal>
			<AlertDialogOverlay
				class="fixed inset-0 z-[10002] bg-black/60 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
			/>
			<AlertDialogContent
				class="fixed top-1/2 left-1/2 z-[10003] grid w-[calc(100%-2rem)] max-w-sm -translate-x-1/2 -translate-y-1/2 gap-4 rounded-lg border bg-background p-6 shadow-lg"
			>
				<AlertDialogHeader>
					<AlertDialogTitle>Delete routine?</AlertDialogTitle>
					<AlertDialogDescription>
						"{{ title }}" and all its occurrences will be deleted. This cannot
						be undone.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Cancel</AlertDialogCancel>
					<AlertDialogAction
						class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
						@click="$emit('confirm')"
					>
						Delete
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialogPortal>
	</AlertDialog>
</template>

<script lang="ts">
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import {
		AlertDialogContent,
		AlertDialogOverlay,
		AlertDialogPortal,
	} from 'radix-vue';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'DeleteRoutineDialog',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogOverlay,
			AlertDialogPortal,
			AlertDialogTitle,
		},
		props: {
			open: { type: Boolean, required: true },
			title: { type: String, required: true },
		},
		emits: ['update:open', 'confirm'],
	});
</script>
