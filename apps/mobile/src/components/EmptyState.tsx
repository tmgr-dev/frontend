import { View, Text } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { tokens } from '../theme';

export function EmptyState({
	icon = 'file-tray-outline',
	title,
	subtitle,
	cta,
}: {
	icon?: any;
	title: string;
	subtitle?: string;
	cta?: string;
}) {
	return (
		<View className="flex-1 items-center justify-center px-10">
			<View className="h-16 w-16 items-center justify-center rounded-card bg-surface">
				<Ionicons name={icon} size={28} color={tokens.inkSubtle} />
			</View>
			<Text className="mt-4 text-center text-base font-semibold text-ink">
				{title}
			</Text>
			{subtitle ? (
				<Text className="mt-1.5 text-center text-sm leading-relaxed text-ink-subtle">
					{subtitle}
				</Text>
			) : null}
			{cta ? (
				<View className="mt-5 rounded-pill bg-brand px-4 py-2.5">
					<Text className="text-sm font-semibold text-white">{cta}</Text>
				</View>
			) : null}
		</View>
	);
}

export function Skeleton({ className = '' }: { className?: string }) {
	return <View className={`rounded-md bg-surface ${className}`} />;
}
