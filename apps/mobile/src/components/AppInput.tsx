import { ReactNode } from 'react';
import { View, Text, TextInput, TextInputProps } from 'react-native';
import { tokens } from '../theme';

type Props = TextInputProps & {
	label: string;
	error?: string;
	right?: ReactNode;
};

export function AppInput({ label, error, right, ...props }: Props) {
	return (
		<View className="mb-4">
			<Text className="mb-1.5 text-2xs font-bold uppercase tracking-wide text-ink-subtle">
				{label}
			</Text>
			<View
				className={`flex-row items-center rounded-md border bg-surface-sunken px-3 ${
					error ? 'border-status-fix' : 'border-line'
				}`}
			>
				<TextInput
					placeholderTextColor={tokens.inkFaint}
					className="h-11 flex-1 text-[15px] text-ink outline-none"
					{...props}
				/>
				{right}
			</View>
			{error ? (
				<Text className="mt-1 text-xs text-status-fix-fg">{error}</Text>
			) : null}
		</View>
	);
}
