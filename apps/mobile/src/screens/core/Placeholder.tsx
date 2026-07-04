import { View, ScrollView, Text } from 'react-native';
import { AppShell } from '../../components/AppShell';
import { TabKey } from '../../components/BottomTabBar';
import { EmptyState, Skeleton } from '../../components/EmptyState';

type Cfg = {
	tab: TabKey;
	heading: string;
	icon: any;
	title: string;
	subtitle: string;
	cta?: string;
};

export function Placeholder({
	cfg,
	loading,
}: {
	cfg: Cfg;
	loading?: boolean;
}) {
	return (
		<AppShell active={cfg.tab}>
			{loading ? (
				<ScrollView contentContainerStyle={{ padding: 16 }}>
					<Skeleton className="mb-4 h-7 w-40" />
					{[0, 1, 2, 3, 4].map((i) => (
						<Skeleton key={i} className="mb-2.5 h-16 w-full" />
					))}
				</ScrollView>
			) : (
				<View className="flex-1">
					<Text className="px-4 pb-1 pt-4 text-xl font-semibold text-ink">
						{cfg.heading}
					</Text>
					<EmptyState
						icon={cfg.icon}
						title={cfg.title}
						subtitle={cfg.subtitle}
						cta={cfg.cta}
					/>
				</View>
			)}
		</AppShell>
	);
}

export const PLACEHOLDERS: Record<string, Cfg> = {
	list: {
		tab: 'list',
		heading: 'Active tasks',
		icon: 'list-outline',
		title: 'No active tasks',
		subtitle: 'Tasks you create or start will show up here. Add one to get going.',
		cta: 'Create a task',
	},
	routines: {
		tab: 'routines',
		heading: 'Daily routines',
		icon: 'repeat-outline',
		title: 'No routines yet',
		subtitle: 'Set up recurring routines to plan your day and build habits.',
		cta: 'New routine',
	},
	categories: {
		tab: 'more',
		heading: 'Categories',
		icon: 'folder-outline',
		title: 'No categories',
		subtitle: 'Group tasks by project or area. Create a category to organise your board.',
		cta: 'New category',
	},
	archive: {
		tab: 'more',
		heading: 'Archive',
		icon: 'archive-outline',
		title: 'Archive is empty',
		subtitle: 'Completed and archived tasks will be kept here for reference.',
	},
};
