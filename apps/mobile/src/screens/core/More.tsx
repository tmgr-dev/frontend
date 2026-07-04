import { View, Text, Pressable, ScrollView } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AppShell } from '../../components/AppShell';
import { tokens } from '../../theme';

const GROUPS: { title: string; items: { icon: any; label: string; hint?: string }[] }[] = [
	{
		title: 'Workspace',
		items: [
			{ icon: 'folder-outline', label: 'Categories', hint: '8' },
			{ icon: 'archive-outline', label: 'Archive' },
			{ icon: 'notifications-outline', label: 'Notifications', hint: '3' },
		],
	},
	{
		title: 'You',
		items: [
			{ icon: 'person-outline', label: 'Profile' },
			{ icon: 'settings-outline', label: 'Settings' },
		],
	},
];

export function More() {
	return (
		<AppShell active="more">
			<ScrollView contentContainerStyle={{ padding: 16 }}>
				<Text className="mb-4 text-xl font-semibold text-ink">More</Text>
				{GROUPS.map((g) => (
					<View key={g.title} className="mb-5">
						<Text className="mb-2 text-2xs font-bold uppercase tracking-wide text-ink-subtle">
							{g.title}
						</Text>
						<View className="overflow-hidden rounded-card border border-line bg-surface">
							{g.items.map((it, i) => (
								<Pressable
									key={it.label}
									className={`flex-row items-center gap-3 px-4 py-3.5 ${
										i ? 'border-t border-line' : ''
									}`}
								>
									<Ionicons name={it.icon} size={19} color={tokens.inkMuted} />
									<Text className="flex-1 text-sm text-ink">{it.label}</Text>
									{it.hint ? (
										<Text className="text-2xs text-ink-subtle">{it.hint}</Text>
									) : null}
									<Ionicons name="chevron-forward" size={16} color={tokens.inkFaint} />
								</Pressable>
							))}
						</View>
					</View>
				))}
			</ScrollView>
		</AppShell>
	);
}
