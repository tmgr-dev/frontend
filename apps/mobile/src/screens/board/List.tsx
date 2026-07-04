import { View, Text, ScrollView, Pressable } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AppShell } from '../../components/AppShell';
import { EmptyState, Skeleton } from '../../components/EmptyState';
import { tokens } from '../../theme';

const AV = {
	ap: { initials: 'AP', color: '#6366f1' },
	sv: { initials: 'SV', color: '#0ea5e9' },
	dk: { initials: 'DK', color: '#22c55e' },
};

type Row = {
	title: string;
	status: string;
	statusColor: string;
	category: string;
	assignee: { initials: string; color: string };
	time: string;
	overtime?: string;
	comments?: number;
	running?: boolean;
};

const ROWS: Row[] = [
	{
		title: 'Redesign onboarding flow for new workspaces',
		status: 'In progress',
		statusColor: tokens.brand,
		category: 'Product',
		assignee: AV.ap,
		time: '2h 14m',
		overtime: '18m',
		comments: 6,
		running: true,
	},
	{
		title: 'Board category filter fix',
		status: 'In progress',
		statusColor: tokens.brand,
		category: 'Bugs',
		assignee: AV.dk,
		time: '48m',
	},
	{
		title: 'Time-log checkpoints',
		status: 'Testing',
		statusColor: tokens.statusTesting,
		category: 'Product',
		assignee: AV.ap,
		time: '1h 05m',
		comments: 3,
	},
	{
		title: 'Ship mobile auth screens',
		status: 'Backlog',
		statusColor: tokens.statusTodo,
		category: 'Mobile',
		assignee: AV.sv,
		time: '—',
	},
];

function TaskRow({ r }: { r: Row }) {
	return (
		<Pressable className="flex-row items-center gap-3 border-t border-line px-4 py-3 first:border-t-0">
			<Pressable className="h-8 w-8 items-center justify-center rounded-pill bg-surface-sunken">
				<Ionicons
					name={r.running ? 'pause' : 'play'}
					size={15}
					color={r.running ? tokens.statusFix : tokens.brand}
				/>
			</Pressable>
			<View className="min-w-0 flex-1">
				<Text className="text-sm text-ink" numberOfLines={1}>
					{r.title}
				</Text>
				<View className="mt-1 flex-row items-center gap-2">
					<View className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: r.statusColor }} />
					<Text className="text-2xs text-ink-subtle">{r.status}</Text>
					<Text className="text-2xs text-ink-faint">·</Text>
					<Text className="text-2xs text-ink-subtle">{r.category}</Text>
					{r.comments ? (
						<>
							<Text className="text-2xs text-ink-faint">·</Text>
							<View className="flex-row items-center gap-0.5">
								<Ionicons name="chatbubble-outline" size={10} color={tokens.inkSubtle} />
								<Text className="text-2xs text-ink-subtle">{r.comments}</Text>
							</View>
						</>
					) : null}
				</View>
			</View>
			<View className="items-end">
				<Text className="font-mono text-2xs tabular-nums text-ink">{r.time}</Text>
				{r.overtime ? (
					<Text className="text-2xs font-semibold text-status-fix-fg">+{r.overtime}</Text>
				) : (
					<View
						className="mt-0.5 h-6 w-6 items-center justify-center rounded-full"
						style={{ backgroundColor: r.assignee.color }}
					>
						<Text className="text-[9px] font-bold text-white">{r.assignee.initials}</Text>
					</View>
				)}
			</View>
		</Pressable>
	);
}

export function List({ loading, empty }: { loading?: boolean; empty?: boolean }) {
	return (
		<AppShell active="list">
			{loading ? (
				<ScrollView contentContainerStyle={{ padding: 16 }}>
					<Skeleton className="mb-4 h-7 w-40" />
					{[0, 1, 2, 3, 4].map((i) => (
						<Skeleton key={i} className="mb-2.5 h-14 w-full" />
					))}
				</ScrollView>
			) : empty ? (
				<EmptyState
					icon="list-outline"
					title="No active tasks"
					subtitle="Tasks you create or start will show up here."
					cta="Create a task"
				/>
			) : (
				<ScrollView contentContainerStyle={{ padding: 16 }}>
					<View className="mb-3 flex-row items-center justify-between">
						<Text className="text-xl font-semibold text-ink">Active tasks</Text>
						<Text className="text-2xs text-ink-subtle">{ROWS.length} tasks · 4h 07m</Text>
					</View>
					<View className="overflow-hidden rounded-card border border-line bg-surface">
						{ROWS.map((r, i) => (
							<TaskRow key={i} r={r} />
						))}
					</View>
				</ScrollView>
			)}
		</AppShell>
	);
}
