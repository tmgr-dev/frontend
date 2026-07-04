import { View, Text, Pressable } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { tokens } from '../../theme';

const WORKSPACES = [
	{ name: 'Acme Product', initial: 'A', color: tokens.brand, members: 6, current: true },
	{ name: 'Personal', initial: 'P', color: '#0ea5e9', members: 1 },
	{ name: 'Side Projects', initial: 'S', color: '#22c55e', members: 3 },
];

export function WorkspaceSwitcher() {
	return (
		<View className="flex-1 justify-end bg-black/50">
			<View className="rounded-t-[22px] border-t border-line bg-surface pb-8 pt-3">
				<View className="mb-3 items-center">
					<View className="h-1 w-10 rounded-full bg-line-strong" />
				</View>
				<Text className="px-5 pb-2 text-2xs font-bold uppercase tracking-wide text-ink-subtle">
					Switch workspace
				</Text>
				{WORKSPACES.map((w) => (
					<Pressable
						key={w.name}
						className="flex-row items-center gap-3 px-5 py-3"
					>
						<View
							className="h-9 w-9 items-center justify-center rounded-md"
							style={{ backgroundColor: w.color }}
						>
							<Text className="text-sm font-black text-white">{w.initial}</Text>
						</View>
						<View className="flex-1">
							<Text className="text-sm font-semibold text-ink">{w.name}</Text>
							<Text className="text-2xs text-ink-subtle">
								{w.members} member{w.members > 1 ? 's' : ''}
							</Text>
						</View>
						{w.current ? (
							<Ionicons name="checkmark-circle" size={20} color={tokens.brand} />
						) : null}
					</Pressable>
				))}
				<View className="mx-5 mt-2 border-t border-line pt-3">
					<Pressable className="flex-row items-center gap-3 py-1">
						<View className="h-9 w-9 items-center justify-center rounded-md border border-dashed border-line-strong">
							<Ionicons name="add" size={18} color={tokens.inkSubtle} />
						</View>
						<Text className="text-sm font-semibold text-brand-fg">
							Create workspace
						</Text>
					</Pressable>
				</View>
			</View>
		</View>
	);
}
