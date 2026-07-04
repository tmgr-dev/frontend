import { Pressable, Text } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { tokens } from '../theme';

const ICON = {
	google: 'logo-google',
	github: 'logo-github',
	apple: 'logo-apple',
} as const;

export function SocialButton({
	provider,
	onPress,
}: {
	provider: keyof typeof ICON;
	onPress?: () => void;
}) {
	const label =
		provider === 'google'
			? 'Continue with Google'
			: provider === 'github'
			? 'Continue with GitHub'
			: 'Continue with Apple';
	return (
		<Pressable
			onPress={onPress}
			className="mb-2.5 h-11 flex-row items-center justify-center gap-2.5 rounded-pill border border-line bg-surface-sunken"
		>
			<Ionicons name={ICON[provider]} size={18} color={tokens.ink} />
			<Text className="text-sm font-medium text-ink">{label}</Text>
		</Pressable>
	);
}
