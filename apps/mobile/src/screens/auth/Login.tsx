import { View } from 'react-native';
import { AuthLayout } from '../../components/AuthLayout';
import { AppInput } from '../../components/AppInput';
import { PasswordInput } from '../../components/PasswordInput';
import { PrimaryButton } from '../../components/PrimaryButton';
import { SocialButton } from '../../components/SocialButton';
import { ErrorBanner, Divider } from '../../components/ErrorBanner';
import { Link, MutedRow } from '../../components/Link';

export function Login({
	loading,
	error,
	filled,
}: {
	loading?: boolean;
	error?: boolean;
	filled?: boolean;
}) {
	return (
		<AuthLayout
			title="Welcome back"
			subtitle="Sign in to your workspace"
			footer={<MutedRow text="New to tmgr?" linkLabel="Create an account" />}
		>
			{error ? <ErrorBanner message="Invalid email or password." /> : null}
			<AppInput
				label="Email"
				placeholder="you@example.com"
				keyboardType="email-address"
				autoCapitalize="none"
				value={filled || error ? 'anna@tmgr.dev' : undefined}
				error={error ? ' ' : undefined}
			/>
			<PasswordInput
				label="Password"
				placeholder="••••••••"
				value={filled || error ? 'password' : undefined}
				error={error ? ' ' : undefined}
			/>
			<View className="mb-4 items-end">
				<Link label="Forgot your password?" />
			</View>
			<PrimaryButton label="Sign in" loading={loading} />
			<Divider label="or" />
			<SocialButton provider="google" />
			<SocialButton provider="github" />
			<SocialButton provider="apple" />
		</AuthLayout>
	);
}
