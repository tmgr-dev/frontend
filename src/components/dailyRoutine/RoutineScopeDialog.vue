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
						$t('dailyRoutine.scopeDialog.title')
					}}</AlertDialogTitle>
					<AlertDialogDescription>
						{{ $t('dailyRoutine.scopeDialog.description') }}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<div class="grid gap-2">
					<template v-for="option in options" :key="option.scope">
						<AlertDialogAction
							:disabled="option.disabled"
							:data-testid="`routine-scope-${option.scope}`"
							class="bg-secondary text-secondary-foreground hover:bg-secondary/80"
							@click="$emit('select', option.scope)"
						>
							{{ $t(`dailyRoutine.scopeDialog.${option.scope}`) }}
						</AlertDialogAction>
						<p
							v-if="option.disabled"
							class="-mt-1 text-xs text-muted-foreground"
						>
							{{ $t('dailyRoutine.scopeDialog.singleDisabled') }}
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
	import { scopeOptions } from './routinePayload';

	export default defineComponent({
		name: 'RoutineScopeDialog',
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
			recurrenceChanged: { type: Boolean, default: false },
		},
		emits: ['update:open', 'select'],
		setup(props) {
			return { options: computed(() => scopeOptions(props.recurrenceChanged)) };
		},
	});
</script>
