import { View, Text, ActivityIndicator } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { PrimaryButton } from '../../components/PrimaryButton';
import { Link } from '../../components/Link';
import { tokens } from '../../theme';

const META = {
	google: { icon: 'logo-google', name: 'Google' },
	github: { icon: 'logo-github', name: 'GitHub' },
	apple: { icon: 'logo-apple', name: 'Apple' },
} as const;

export function SocialConnecting({
	provider = 'google',
	error,
}: {
	provider?: keyof typeof META;
	error?: boolean;
}) {
	const m = META[provider];
	return (
		<View className="flex-1 items-center justify-center bg-surface-sunken px-6">
			<View className="w-full max-w-[400px] items-center rounded-card border border-line bg-surface p-8">
				<View className="h-16 w-16 items-center justify-center rounded-card bg-surface-sunken">
					<Ionicons name={m.icon} size={30} color={tokens.ink} />
				</View>
				{error ? (
					<>
						<Text className="mt-5 text-lg font-semibold text-ink">
							Couldn’t connect to {m.name}
						</Text>
						<Text className="mt-1.5 text-center text-sm leading-relaxed text-ink-subtle">
							The sign-in was cancelled or timed out. Please try again.
						</Text>
						<View className="mt-6 w-full">
							<PrimaryButton label={`Retry with ${m.name}`} />
							<View className="mt-4 items-center">
								<Link label="Back to sign in" />
							</View>
						</View>
					</>
				) : (
					<>
						<Text className="mt-5 text-lg font-semibold text-ink">
							Connecting to {m.name}…
						</Text>
						<Text className="mt-1.5 text-center text-sm leading-relaxed text-ink-subtle">
							Continue in the window that just opened to finish signing in.
						</Text>
						<View className="mt-6">
							<ActivityIndicator color={tokens.brand} />
						</View>
					</>
				)}
			</View>
		</View>
	);
}
