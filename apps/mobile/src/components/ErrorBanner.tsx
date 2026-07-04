import { View, Text } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { tokens } from '../theme';

export function ErrorBanner({ message }: { message: string }) {
	return (
		<View className="mb-4 flex-row items-center gap-2 rounded-md border border-status-fix bg-status-fix-bg px-3 py-2.5">
			<Ionicons name="alert-circle" size={16} color={tokens.statusFixFg} />
			<Text className="flex-1 text-xs font-medium text-status-fix-fg">
				{message}
			</Text>
		</View>
	);
}

export function Divider({ label }: { label: string }) {
	return (
		<View className="my-4 flex-row items-center gap-3">
			<View className="h-px flex-1 bg-line" />
			<Text className="text-2xs uppercase tracking-wide text-ink-faint">
				{label}
			</Text>
			<View className="h-px flex-1 bg-line" />
		</View>
	);
}
