import { useState } from 'react';
import { Pressable, TextInputProps } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { AppInput } from './AppInput';
import { tokens } from '../theme';

type Props = TextInputProps & { label: string; error?: string };

export function PasswordInput({ label, error, ...props }: Props) {
	const [hidden, setHidden] = useState(true);
	return (
		<AppInput
			label={label}
			error={error}
			secureTextEntry={hidden}
			autoCapitalize="none"
			right={
				<Pressable onPress={() => setHidden((h) => !h)} className="pl-2">
					<Ionicons
						name={hidden ? 'eye-outline' : 'eye-off-outline'}
						size={18}
						color={tokens.inkSubtle}
					/>
				</Pressable>
			}
			{...props}
		/>
	);
}
