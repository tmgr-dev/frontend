import { View, Text, ActivityIndicator } from 'react-native';
import { Logo } from '../../components/Logo';
import { tokens } from '../../theme';

export function Splash() {
	return (
		<View className="flex-1 items-center justify-center bg-surface-sunken">
			<View className="items-center">
				<Logo size="lg" />
				<Text className="mt-3 text-sm text-ink-subtle">
					Focused task & time tracking
				</Text>
			</View>
			<View className="absolute bottom-24">
				<ActivityIndicator color={tokens.brand} />
			</View>
		</View>
	);
}
