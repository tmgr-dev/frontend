import { View, Text } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AuthLayout } from '../../components/AuthLayout';
import { AppInput } from '../../components/AppInput';
import { PrimaryButton } from '../../components/PrimaryButton';
import { ErrorBanner } from '../../components/ErrorBanner';
import { Link } from '../../components/Link';
import { tokens } from '../../theme';

export function Forgot({
	loading,
	success,
	error,
}: {
	loading?: boolean;
	success?: boolean;
	error?: boolean;
}) {
	if (success) {
		return (
			<AuthLayout
				title="Check your email"
				footer={<Link label="Back to sign in" />}
			>
				<View className="items-center py-2">
					<View className="h-12 w-12 items-center justify-center rounded-pill bg-status-done-bg">
						<Ionicons name="mail-outline" size={22} color={tokens.statusDone} />
					</View>
					<Text className="mt-4 text-center text-sm leading-relaxed text-ink-subtle">
						We sent a reset link to{' '}
						<Text className="font-semibold text-ink">anna@tmgr.dev</Text>. Follow
						it to choose a new password.
					</Text>
				</View>
			</AuthLayout>
		);
	}
	return (
		<AuthLayout
			title="Reset your password"
			subtitle="Enter the email tied to your account and we'll send a reset link."
			footer={<Link label="Back to sign in" />}
		>
			{error ? <ErrorBanner message="No account found for that email." /> : null}
			<AppInput
				label="Email"
				placeholder="you@example.com"
				keyboardType="email-address"
				autoCapitalize="none"
				value={error ? 'ghost@tmgr.dev' : undefined}
				error={error ? ' ' : undefined}
			/>
			<PrimaryButton label="Send reset link" loading={loading} />
		</AuthLayout>
	);
}
