import { View, Text, Pressable } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { tokens } from '../theme';

export type TabKey = 'dashboard' | 'board' | 'list' | 'routines' | 'more';

const TABS: { key: TabKey; icon: any; label: string }[] = [
	{ key: 'dashboard', icon: 'home-outline', label: 'Home' },
	{ key: 'board', icon: 'grid-outline', label: 'Board' },
	{ key: 'list', icon: 'list-outline', label: 'List' },
	{ key: 'routines', icon: 'repeat-outline', label: 'Routines' },
	{ key: 'more', icon: 'ellipsis-horizontal', label: 'More' },
];

export function BottomTabBar({ active }: { active: TabKey }) {
	return (
		<View className="flex-row border-t border-line bg-surface pb-5 pt-2">
			{TABS.map((t) => {
				const on = t.key === active;
				return (
					<Pressable key={t.key} className="flex-1 items-center gap-0.5 py-1">
						<Ionicons
							name={on ? t.icon.replace('-outline', '') : t.icon}
							size={22}
							color={on ? tokens.brand : tokens.inkSubtle}
						/>
						<Text
							className={`text-2xs ${on ? 'font-semibold text-brand' : 'text-ink-subtle'}`}
						>
							{t.label}
						</Text>
					</Pressable>
				);
			})}
		</View>
	);
}
