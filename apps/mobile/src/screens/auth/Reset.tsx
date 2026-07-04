import { View, Text } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AuthLayout } from '../../components/AuthLayout';
import { PasswordInput } from '../../components/PasswordInput';
import { PrimaryButton } from '../../components/PrimaryButton';
import { ErrorBanner } from '../../components/ErrorBanner';
import { Link } from '../../components/Link';
import { tokens } from '../../theme';

export function Reset({
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
			<AuthLayout title="Password updated" footer={<Link label="Back to sign in" />}>
				<View className="items-center py-2">
					<View className="h-12 w-12 items-center justify-center rounded-pill bg-status-done-bg">
						<Ionicons
							name="checkmark-circle"
							size={24}
							color={tokens.statusDone}
						/>
					</View>
					<Text className="mt-4 text-center text-sm leading-relaxed text-ink-subtle">
						Your password has been changed. You can now sign in with your new
						password.
					</Text>
				</View>
			</AuthLayout>
		);
	}
	return (
		<AuthLayout
			title="Set a new password"
			subtitle="Choose a strong password you don't use elsewhere."
			footer={<Link label="Back to sign in" />}
		>
			{error ? (
				<ErrorBanner message="This reset link has expired. Request a new one." />
			) : null}
			<PasswordInput label="New password" placeholder="At least 8 characters" />
			<PasswordInput
				label="Confirm new password"
				placeholder="Repeat your password"
			/>
			<PrimaryButton label="Reset password" loading={loading} />
		</AuthLayout>
	);
}
