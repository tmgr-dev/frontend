<template>
	<DropdownMenu>
		<DropdownMenuTrigger as-child>
			<button
				type="button"
				class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink dark:hover:bg-gray-800"
				aria-label="Help and tours"
				data-testid="help-menu"
			>
				<CircleQuestionMark class="h-4 w-4" />
			</button>
		</DropdownMenuTrigger>
		<DropdownMenuContent align="end" :side-offset="8" class="min-w-56">
			<DropdownMenuItem
				v-if="workspaceCode"
				class="cursor-pointer"
				@click="$router.push(`/${workspaceCode}/getting-started`)"
			>
				<LifeBuoy />
				Getting started
			</DropdownMenuItem>
			<DropdownMenuItem
				v-if="canRestart"
				class="cursor-pointer"
				@click="restart"
			>
				<RotateCcw />
				Restart this page's tour
			</DropdownMenuItem>
			<DropdownMenuItem
				class="cursor-pointer"
				@click="$router.push('/settings?tab=about')"
			>
				<SlidersHorizontal />
				Tips settings
			</DropdownMenuItem>
		</DropdownMenuContent>
	</DropdownMenu>
</template>

<script lang="ts">
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { tourForRoute } from '@/tours/hubModel';
	import { useTours } from '@/tours/useTours';
	import {
		CircleQuestionMark,
		LifeBuoy,
		RotateCcw,
		SlidersHorizontal,
	} from 'lucide-vue-next';
	import { computed, defineComponent } from 'vue';
	import { useRoute } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'HelpMenu',
		components: {
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuTrigger,
			CircleQuestionMark,
			LifeBuoy,
			RotateCcw,
			SlidersHorizontal,
		},
		setup() {
			const route = useRoute();
			const store = useStore();
			const tours = useTours();

			const workspaceCode = computed(
				() => store.getters.currentWorkspace?.code ?? null,
			);
			const pageTour = computed(() =>
				tourForRoute(tours.hubItems.value, route.name as string | undefined),
			);

			const canRestart = computed(
				() => !!pageTour.value && tours.hostReady.value,
			);

			return {
				workspaceCode,
				pageTour,
				canRestart,
				restart: () => {
					if (pageTour.value) void tours.start(pageTour.value.tour.id, 'hub');
				},
			};
		},
	});
</script>
