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
					<AlertDialogTitle>{{
						$t('dailyRoutine.deleteDialog.title')
					}}</AlertDialogTitle>
					<AlertDialogDescription>
						{{
							recurring
								? $t('dailyRoutine.deleteDialog.recurringDescription', {
										title,
									})
								: $t('dailyRoutine.deleteDialog.description', { title })
						}}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<div class="grid gap-2">
					<template v-for="choice in choices" :key="choice.scope">
						<AlertDialogAction
							:data-testid="`routine-delete-${choice.scope}`"
							:class="
								choice.scope === 'all'
									? 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
									: 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
							"
							@click="$emit('confirm', choice.scope)"
						>
							{{
								recurring
									? $t(`dailyRoutine.deleteDialog.${choice.scope}`)
									: $t('common.actions.delete')
							}}
						</AlertDialogAction>
						<p
							v-if="choice.warnComments"
							class="-mt-1 text-xs text-destructive"
							data-testid="routine-delete-comments-warning"
						>
							{{
								$t(
									'dailyRoutine.deleteDialog.commentsWarning',
									{ n: commentsCount },
									commentsCount,
								)
							}}
						</p>
					</template>
				</div>
				<AlertDialogFooter>
					<AlertDialogCancel>{{
						$t('common.actions.cancel')
					}}</AlertDialogCancel>
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
	import { computed, defineComponent } from 'vue';
	import { deleteChoices } from './routinePayload';

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
			recurring: { type: Boolean, default: false },
			commentsCount: { type: Number, default: 0 },
		},
		emits: ['update:open', 'confirm'],
		setup(props) {
			return {
				choices: computed(() =>
					deleteChoices(props.recurring, props.commentsCount),
				),
			};
		},
	});
</script>
