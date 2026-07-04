import { ReactNode } from 'react';
import { View, Text, Pressable } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { BottomTabBar, TabKey } from './BottomTabBar';
import { tokens } from '../theme';

export function AppShell({
	active,
	workspace = 'Acme Product',
	children,
}: {
	active: TabKey;
	workspace?: string;
	children: ReactNode;
}) {
	return (
		<View className="flex-1 bg-surface-sunken">
			{/* Header */}
			<View className="flex-row items-center gap-2 border-b border-line bg-surface px-4 pb-3 pt-14">
				<Pressable className="flex-row items-center gap-2 rounded-pill bg-surface-sunken px-2.5 py-1.5">
					<View className="h-6 w-6 items-center justify-center rounded-md bg-brand">
						<Text className="text-xs font-black text-white">A</Text>
					</View>
					<Text className="max-w-[140px] text-sm font-semibold text-ink" numberOfLines={1}>
						{workspace}
					</Text>
					<Ionicons name="chevron-down" size={15} color={tokens.inkSubtle} />
				</Pressable>
				<View className="ml-auto flex-row items-center gap-1">
					<Pressable className="h-9 w-9 items-center justify-center rounded-pill">
						<Ionicons name="notifications-outline" size={20} color={tokens.inkSubtle} />
					</Pressable>
					<View className="h-8 w-8 items-center justify-center rounded-pill bg-[#6366f1]">
						<Text className="text-xs font-bold text-white">AP</Text>
					</View>
				</View>
			</View>

			<View className="flex-1">{children}</View>

			<BottomTabBar active={active} />
		</View>
	);
}
