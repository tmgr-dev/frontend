import { View, Text, ScrollView, Pressable } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AppShell } from '../../components/AppShell';
import { BoardCard, Task } from '../../components/BoardCard';
import { EmptyState, Skeleton } from '../../components/EmptyState';
import { tokens } from '../../theme';

const AV = {
	ap: { initials: 'AP', color: '#6366f1' },
	sv: { initials: 'SV', color: '#0ea5e9' },
	dk: { initials: 'DK', color: '#22c55e' },
};

const COLUMNS: {
	name: string;
	color: string;
	textClass: string;
	tasks: Task[];
}[] = [
	{
		name: 'Backlog',
		color: tokens.statusTodo,
		textClass: 'text-status-todo',
		tasks: [
			{ title: 'Ship mobile auth screens', category: 'Mobile', assignee: AV.sv },
			{ title: 'Write onboarding copy', category: 'Content', comments: 2 },
		],
	},
	{
		name: 'In progress',
		color: tokens.brand,
		textClass: 'text-brand',
		tasks: [
			{
				title: 'Redesign onboarding flow for new workspaces',
				category: 'Product',
				assignee: AV.ap,
				time: '2h 14m',
				overtime: '18m',
				comments: 6,
			},
			{ title: 'Board category filter fix', category: 'Bugs', assignee: AV.dk, time: '48m' },
		],
	},
	{
		name: 'Testing',
		color: tokens.statusTesting,
		textClass: 'text-status-testing',
		tasks: [{ title: 'Time-log checkpoints', category: 'Product', assignee: AV.ap, comments: 3 }],
	},
];

const CATS = ['All', 'Product', 'Mobile', 'Bugs', 'Content'];

function Column({ col, filter }: { col: (typeof COLUMNS)[0]; filter: string }) {
	const tasks =
		filter === 'All' ? col.tasks : col.tasks.filter((t) => t.category === filter);
	return (
		<View className="w-[300px]">
			<View className="mb-2.5 flex-row items-center gap-2 px-0.5">
				<View
					className="h-2 w-2 rounded-full"
					style={{ backgroundColor: col.color }}
				/>
				<Text
					className={`text-2xs font-bold uppercase tracking-wide ${col.textClass}`}
				>
					{col.name}
				</Text>
				<Text className="text-2xs tabular-nums text-ink-subtle">{tasks.length}</Text>
			</View>
			{tasks.map((t, i) => (
				<BoardCard key={i} task={t} />
			))}
			<Pressable className="mt-0.5 flex-row items-center justify-center gap-1 rounded-card border border-dashed border-line-strong py-2">
				<Ionicons name="add" size={14} color={tokens.inkSubtle} />
				<Text className="text-2xs font-medium text-ink-subtle">Add task</Text>
			</Pressable>
		</View>
	);
}

export function Board({
	loading,
	empty,
	filter = 'All',
}: {
	loading?: boolean;
	empty?: boolean;
	filter?: string;
}) {
	return (
		<AppShell active="board">
			{/* Category filter chips */}
			<View className="border-b border-line bg-surface-sunken">
				<ScrollView
					horizontal
					showsHorizontalScrollIndicator={false}
					contentContainerStyle={{ paddingHorizontal: 16, paddingVertical: 10, gap: 8 }}
				>
					{CATS.map((c) => {
						const on = c === filter;
						return (
							<Pressable
								key={c}
								className={`flex-row items-center gap-1 rounded-pill border px-3 py-1.5 ${
									on ? 'border-brand bg-brand-bg' : 'border-line bg-surface'
								}`}
							>
								{c === 'All' ? (
									<Ionicons
										name="funnel-outline"
										size={12}
										color={on ? tokens.brandFg : tokens.inkSubtle}
									/>
								) : null}
								<Text
									className={`text-xs font-medium ${on ? 'text-brand-fg' : 'text-ink-subtle'}`}
								>
									{c}
								</Text>
							</Pressable>
						);
					})}
				</ScrollView>
			</View>

			{loading ? (
				<ScrollView horizontal contentContainerStyle={{ padding: 16, gap: 16 }}>
					{[0, 1].map((c) => (
						<View key={c} className="w-[300px]">
							<Skeleton className="mb-2.5 h-5 w-24" />
							{[0, 1, 2].map((i) => (
								<Skeleton key={i} className="mb-2.5 h-24 w-full" />
							))}
						</View>
					))}
				</ScrollView>
			) : empty ? (
				<EmptyState
					icon="grid-outline"
					title="This board is empty"
					subtitle="Add a status column and your first task to start organising work."
					cta="Add a task"
				/>
			) : (
				<ScrollView
					horizontal
					showsHorizontalScrollIndicator={false}
					contentContainerStyle={{ paddingHorizontal: 16, paddingTop: 14, gap: 16 }}
				>
					{COLUMNS.map((col) => (
						<Column key={col.name} col={col} filter={filter} />
					))}
				</ScrollView>
			)}
		</AppShell>
	);
}
