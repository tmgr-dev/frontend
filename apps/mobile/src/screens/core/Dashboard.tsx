import { View, Text, ScrollView } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AppShell } from '../../components/AppShell';
import { EmptyState, Skeleton } from '../../components/EmptyState';
import { tokens } from '../../theme';

const STATS = [
	{ label: 'Logged today', value: '2h 14m', icon: 'time-outline' },
	{ label: 'Tasks done', value: '5', icon: 'checkmark-done-outline' },
	{ label: 'In progress', value: '3', icon: 'ellipse-outline' },
];

const RECENT = [
	{ title: 'Redesign onboarding flow', status: 'In progress', color: tokens.brand, time: '2h 14m' },
	{ title: 'Fix board category filter', status: 'Testing', color: '#8b5cf6', time: '48m' },
	{ title: 'Ship mobile auth screens', status: 'Backlog', color: '#0ea5e9', time: '—' },
];

export function Dashboard({
	loading,
	empty,
}: {
	loading?: boolean;
	empty?: boolean;
}) {
	return (
		<AppShell active="dashboard">
			{loading ? (
				<ScrollView contentContainerStyle={{ padding: 16 }}>
					<Skeleton className="mb-4 h-7 w-48" />
					<View className="mb-4 flex-row gap-3">
						<Skeleton className="h-20 flex-1" />
						<Skeleton className="h-20 flex-1" />
						<Skeleton className="h-20 flex-1" />
					</View>
					<Skeleton className="mb-2 h-4 w-24" />
					{[0, 1, 2].map((i) => (
						<Skeleton key={i} className="mb-2 h-16 w-full" />
					))}
				</ScrollView>
			) : empty ? (
				<EmptyState
					icon="sparkles-outline"
					title="Nothing tracked yet"
					subtitle="Create your first task to start tracking time and see your dashboard fill up."
					cta="Create a task"
				/>
			) : (
				<ScrollView contentContainerStyle={{ padding: 16 }}>
					<Text className="text-xl font-semibold text-ink">Good morning, Anna</Text>
					<Text className="mt-1 text-sm text-ink-subtle">
						Here's your day at a glance
					</Text>

					<View className="mt-4 flex-row gap-3">
						{STATS.map((s) => (
							<View
								key={s.label}
								className="flex-1 rounded-card border border-line bg-surface p-3"
							>
								<Ionicons name={s.icon as any} size={16} color={tokens.inkSubtle} />
								<Text className="mt-2 text-lg font-bold text-ink">{s.value}</Text>
								<Text className="text-2xs text-ink-subtle">{s.label}</Text>
							</View>
						))}
					</View>

					<Text className="mb-2 mt-6 text-2xs font-bold uppercase tracking-wide text-ink-subtle">
						Recent tasks
					</Text>
					<View className="mt-2 overflow-hidden rounded-card border border-line bg-surface">
						{RECENT.map((t, i) => (
							<View
								key={t.title}
								className={`flex-row items-center gap-3 px-4 py-3 ${
									i ? 'border-t border-line' : ''
								}`}
							>
								<View
									className="h-2 w-2 rounded-full"
									style={{ backgroundColor: t.color }}
								/>
								<View className="flex-1">
									<Text className="text-sm text-ink" numberOfLines={1}>
										{t.title}
									</Text>
									<Text className="text-2xs text-ink-subtle">{t.status}</Text>
								</View>
								<Text className="font-mono text-2xs tabular-nums text-ink-subtle">
									{t.time}
								</Text>
							</View>
						))}
					</View>
				</ScrollView>
			)}
		</AppShell>
	);
}
