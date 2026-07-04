import { AuthLayout } from '../../components/AuthLayout';
import { AppInput } from '../../components/AppInput';
import { PasswordInput } from '../../components/PasswordInput';
import { PrimaryButton } from '../../components/PrimaryButton';
import { SocialButton } from '../../components/SocialButton';
import { Divider } from '../../components/ErrorBanner';
import { MutedRow } from '../../components/Link';

export function Register({
	loading,
	validation,
}: {
	loading?: boolean;
	validation?: boolean;
}) {
	return (
		<AuthLayout
			title="Create your account"
			subtitle="Start tracking your work in minutes"
			footer={<MutedRow text="Already have an account?" linkLabel="Sign in" />}
		>
			<AppInput
				label="Name"
				placeholder="Your name"
				value={validation ? 'A' : undefined}
			/>
			<AppInput
				label="Email"
				placeholder="you@example.com"
				keyboardType="email-address"
				autoCapitalize="none"
				value={validation ? 'not-an-email' : undefined}
				error={validation ? 'Enter a valid email address.' : undefined}
			/>
			<PasswordInput
				label="Password"
				placeholder="At least 8 characters"
				value={validation ? '123' : undefined}
				error={validation ? 'Password must be at least 8 characters.' : undefined}
			/>
			<PasswordInput
				label="Confirm password"
				placeholder="Repeat your password"
				value={validation ? '1234' : undefined}
				error={validation ? "Passwords don't match." : undefined}
			/>
			<PrimaryButton label="Create account" loading={loading} />
			<Divider label="or" />
			<SocialButton provider="google" />
			<SocialButton provider="github" />
			<SocialButton provider="apple" />
		</AuthLayout>
	);
}
