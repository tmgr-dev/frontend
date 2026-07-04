import { Pressable, Text, ActivityIndicator, View } from 'react-native';
import { tokens } from '../theme';

export function PrimaryButton({
	label,
	loading,
	onPress,
}: {
	label: string;
	loading?: boolean;
	onPress?: () => void;
}) {
	return (
		<Pressable
			disabled={loading}
			onPress={onPress}
			className={`h-11 flex-row items-center justify-center rounded-pill bg-brand ${
				loading ? 'opacity-80' : ''
			}`}
		>
			{loading ? (
				<View className="flex-row items-center gap-2">
					<ActivityIndicator size="small" color={tokens.white} />
					<Text className="text-sm font-semibold text-white">Please wait…</Text>
				</View>
			) : (
				<Text className="text-sm font-semibold text-white">{label}</Text>
			)}
		</Pressable>
	);
}
