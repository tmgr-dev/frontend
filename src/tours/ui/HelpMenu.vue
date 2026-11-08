<template>
	<DropdownMenu>
		<DropdownMenuTrigger as-child>
			<button
				type="button"
				class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink"
				:aria-label="$t('onboarding.tours.help.label')"
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
				{{ $t('onboarding.tours.help.gettingStarted') }}
			</DropdownMenuItem>
			<DropdownMenuItem
				v-if="isDesktop"
				class="cursor-pointer"
				data-testid="watch-demo"
				@click="watchDemoClick"
			>
				<Play />
				{{ $t('onboarding.tours.help.watchDemo') }}
			</DropdownMenuItem>
			<DropdownMenuItem
				v-if="canRestart"
				class="cursor-pointer"
				@click="restart"
			>
				<RotateCcw />
				{{ $t('onboarding.tours.help.restartPageTour') }}
			</DropdownMenuItem>
			<DropdownMenuItem
				class="cursor-pointer"
				@click="$router.push('/settings?tab=about')"
			>
				<SlidersHorizontal />
				{{ $t('onboarding.tours.help.tipsSettings') }}
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
	import { isDesktopApp } from '@/utils/desktop';
	import {
		CircleQuestionMark,
		LifeBuoy,
		Play,
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
			Play,
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

			const watchDemoClick = async () => {
				try {
					const { watchDemo } = await import('@/local/demoApi');
					await watchDemo();
				} catch (error) {
					console.error('Could not open the demo workspace', error);
				}
			};

			return {
				isDesktop: isDesktopApp(),
				watchDemoClick,
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
