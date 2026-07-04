import { View, Text, Pressable, ScrollView } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AppShell } from '../../components/AppShell';
import { tokens } from '../../theme';

function Row({
	icon,
	label,
	value,
	danger,
}: {
	icon: any;
	label: string;
	value?: string;
	danger?: boolean;
}) {
	return (
		<Pressable className="flex-row items-center gap-3 border-t border-line px-4 py-3.5 first:border-t-0">
			<Ionicons
				name={icon}
				size={19}
				color={danger ? tokens.statusFix : tokens.inkMuted}
			/>
			<Text className={`flex-1 text-sm ${danger ? 'text-status-fix-fg' : 'text-ink'}`}>
				{label}
			</Text>
			{value ? <Text className="text-2xs text-ink-subtle">{value}</Text> : null}
			{!danger ? (
				<Ionicons name="chevron-forward" size={16} color={tokens.inkFaint} />
			) : null}
		</Pressable>
	);
}

export function Settings() {
	return (
		<AppShell active="more">
			<ScrollView contentContainerStyle={{ padding: 16 }}>
				<Text className="mb-4 text-xl font-semibold text-ink">Settings</Text>

				<View className="mb-4 flex-row items-center gap-3 rounded-card border border-line bg-surface p-4">
					<View className="h-12 w-12 items-center justify-center rounded-pill bg-[#6366f1]">
						<Text className="text-base font-bold text-white">AP</Text>
					</View>
					<View className="flex-1">
						<Text className="text-base font-semibold text-ink">Anna Petrova</Text>
						<Text className="text-2xs text-ink-subtle">anna@tmgr.dev</Text>
					</View>
					<Ionicons name="chevron-forward" size={16} color={tokens.inkFaint} />
				</View>

				<Text className="mb-2 text-2xs font-bold uppercase tracking-wide text-ink-subtle">
					Appearance
				</Text>
				<View className="mb-5 overflow-hidden rounded-card border border-line bg-surface">
					<Row icon="moon-outline" label="Theme" value="Dark" />
					<Row icon="color-palette-outline" label="Accent" value="Coral" />
				</View>

				<Text className="mb-2 text-2xs font-bold uppercase tracking-wide text-ink-subtle">
					Preferences
				</Text>
				<View className="mb-5 overflow-hidden rounded-card border border-line bg-surface">
					<Row icon="notifications-outline" label="Notifications" value="On" />
					<Row icon="timer-outline" label="Pomodoro" value="25 / 5" />
					<Row icon="language-outline" label="Language" value="English" />
				</View>

				<View className="overflow-hidden rounded-card border border-line bg-surface">
					<Row icon="log-out-outline" label="Sign out" danger />
				</View>
			</ScrollView>
		</AppShell>
	);
}
