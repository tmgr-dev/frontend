import { View, Text } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { tokens } from '../theme';

export type Task = {
	title: string;
	category?: string;
	assignee?: { initials: string; color: string };
	time?: string;
	comments?: number;
	overtime?: string;
};

export function BoardCard({ task }: { task: Task }) {
	return (
		<View className="mb-2.5 rounded-card border border-line bg-surface p-3">
			<Text className="text-sm leading-snug text-ink">{task.title}</Text>
			{task.category ? (
				<View className="mt-2 flex-row">
					<Text className="rounded-pill bg-surface-sunken px-2 py-0.5 text-2xs font-medium text-ink-subtle">
						{task.category}
					</Text>
				</View>
			) : null}
			<View className="mt-2.5 flex-row items-center gap-3">
				{task.assignee ? (
					<View
						className="h-6 w-6 items-center justify-center rounded-full"
						style={{ backgroundColor: task.assignee.color }}
					>
						<Text className="text-[10px] font-bold text-white">
							{task.assignee.initials}
						</Text>
					</View>
				) : null}
				{task.time ? (
					<Text className="font-mono text-2xs tabular-nums text-ink-subtle">
						{task.time}
					</Text>
				) : null}
				{task.overtime ? (
					<View className="flex-row items-center gap-1">
						<Ionicons name="alert-circle" size={12} color={tokens.statusFixFg} />
						<Text className="text-2xs font-semibold text-status-fix-fg">
							+{task.overtime}
						</Text>
					</View>
				) : null}
				{task.comments ? (
					<View className="ml-auto flex-row items-center gap-1">
						<Ionicons name="chatbubble-outline" size={12} color={tokens.inkSubtle} />
						<Text className="text-2xs tabular-nums text-ink-subtle">
							{task.comments}
						</Text>
					</View>
				) : null}
			</View>
		</View>
	);
}
