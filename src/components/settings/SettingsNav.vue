<template>
	<nav
		aria-label="Settings"
		data-testid="settings-nav"
		class="flex flex-col"
		:class="variant === 'list' ? 'gap-6' : 'gap-5'"
	>
		<div v-for="group in groups" :key="group.key">
			<p
				:id="`settings-nav-${group.key}`"
				class="mb-1 text-2xs font-semibold tracking-wide text-ink-subtle uppercase dark:text-ink-subtle"
				:class="variant === 'list' ? 'px-4' : 'px-2'"
			>
				{{ group.label }}
			</p>
			<ul
				:aria-labelledby="`settings-nav-${group.key}`"
				:class="
					variant === 'list'
						? 'divide-y divide-border dark:divide-border overflow-hidden rounded-lg border border-border bg-card dark:border-border dark:bg-card'
						: 'flex flex-col gap-0.5'
				"
			>
				<li v-for="item in group.items" :key="item.key">
					<router-link v-slot="{ href, navigate }" :to="location(item)" custom>
						<a
							:href="href"
							:aria-current="isActive(item) ? 'page' : undefined"
							:data-active="isActive(item)"
							:data-testid="`settings-nav-${item.key}`"
							:class="variant === 'list' ? LIST_ROW : COLUMN_ROW"
							@click="navigate"
						>
							<component :is="icon(item.icon)" class="size-4 shrink-0" />
							<span class="min-w-0 flex-1 truncate">{{ item.label }}</span>
							<ChevronRight
								v-if="variant === 'list'"
								class="size-4 shrink-0 text-ink-subtle dark:text-ink-subtle"
								aria-hidden="true"
							/>
						</a>
					</router-link>
				</li>
			</ul>
		</div>
	</nav>
</template>

<script lang="ts">
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { isRunnerSupported } from '@/runner/api';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		isSettingsNavItemActive,
		settingsNavGroups,
		settingsNavLocation,
		type SettingsNavItem,
	} from '@/utils/settingsNav';
	import {
		BadgeCheck,
		Bell,
		Bot,
		Cable,
		ChevronRight,
		FlaskConical,
		HardDrive,
		Info,
		Keyboard,
		Palette,
		Plug,
		Settings2,
		ShieldCheck,
		Sliders,
		Smartphone,
		VenetianMask,
		type LucideIcon,
	} from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';
	import { useRoute } from 'vue-router';
	import { useStore } from 'vuex';

	const ICONS: Record<string, LucideIcon> = {
		profile: BadgeCheck,
		notifications: Bell,
		devices: Cable,
		privacy: ShieldCheck,
		workspace: Settings2,
		modules: Sliders,
		personas: VenetianMask,
		'agent-work': Bot,
		alerts: Smartphone,
		plugins: Plug,
		theme: Palette,
		shortcuts: Keyboard,
		'local-data': HardDrive,
		experimental: FlaskConical,
		about: Info,
	};

	const COLUMN_ROW =
		'flex h-8 items-center gap-2 rounded-md px-2 text-sm text-ink-muted outline-hidden transition-colors hover:bg-surface-hover hover:text-ink focus-visible:ring-2 focus-visible:ring-sidebar-ring data-[active=true]:bg-sidebar-primary/10 data-[active=true]:font-medium data-[active=true]:text-ink dark:text-ink-muted dark:hover:bg-surface-hover dark:hover:text-ink dark:data-[active=true]:bg-sidebar-primary/15 dark:data-[active=true]:text-ink [&[data-active=true]>svg]:text-sidebar-primary';

	const LIST_ROW =
		'flex min-h-11 items-center gap-3 px-4 py-2.5 text-sm text-ink outline-hidden transition-colors hover:bg-surface-hover focus-visible:bg-surface-hover active:bg-surface-hover dark:text-ink dark:hover:bg-surface-hover [&>svg:first-child]:text-ink-subtle';

	export default defineComponent({
		name: 'SettingsNav',
		components: { ChevronRight },
		props: {
			variant: {
				type: String as PropType<'column' | 'list'>,
				default: 'column',
			},
		},
		setup() {
			const route = useRoute();
			const store = useStore();
			const { isFeatureEnabled } = useFeatureToggles();
			const isDesktop = isDesktopApp();

			const groups = computed(() => {
				const isLocalOnly = store.getters.isLocalOnly === true;
				return settingsNavGroups({
					isLocalOnly,
					isDesktop,
					hasLocalWorkspaces:
						isDesktop &&
						(isLocalOnly ||
							(store.state.workspaces || []).some(
								(w: { is_local?: boolean }) => w.is_local,
							)),
					runnerSupported: isRunnerSupported(),
					isFeatureEnabled,
				});
			});

			const isActive = (item: SettingsNavItem) =>
				isSettingsNavItemActive(item, route);

			return {
				groups,
				isActive,
				location: settingsNavLocation,
				icon: (key: string) => ICONS[key],
				COLUMN_ROW,
				LIST_ROW,
			};
		},
	});
</script>
