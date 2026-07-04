import { View, Text } from 'react-native';

export function Logo({ size = 'md' }: { size?: 'md' | 'lg' }) {
	const box = size === 'lg' ? 'h-14 w-14' : 'h-11 w-11';
	const mark = size === 'lg' ? 'text-2xl' : 'text-xl';
	const word = size === 'lg' ? 'text-2xl' : 'text-lg';
	return (
		<View className="flex-row items-center gap-2.5">
			<View
				className={`${box} items-center justify-center rounded-card bg-brand`}
			>
				<Text className={`${mark} font-black text-white`}>t</Text>
			</View>
			<Text className={`${word} font-bold tracking-tight text-ink`}>
				tmgr<Text className="text-brand">.dev</Text>
			</Text>
		</View>
	);
}
