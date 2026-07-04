import './global.css';
import { View, Text, ScrollView, Pressable } from 'react-native';
import { registry, registryKeys } from './src/registry';

function getScreenKey(): string | null {
	if (typeof window === 'undefined') return null;
	return new URLSearchParams(window.location.search).get('s');
}

export default function App() {
	const key = getScreenKey();
	if (key && registry[key]) {
		return registry[key];
	}
	// Index (dev only): list of available screen/state keys.
	return (
		<ScrollView className="flex-1 bg-surface-sunken">
			<View className="px-6 py-12">
				<Text className="mb-4 text-lg font-bold text-ink">
					TMGR mobile — AUTH batch
				</Text>
				{registryKeys.map((k) => (
					<Pressable
						key={k}
						onPress={() => {
							if (typeof window !== 'undefined')
								window.location.search = `?s=${k}`;
						}}
						className="mb-2 rounded-md border border-line bg-surface px-3 py-2"
					>
						<Text className="text-sm text-ink">{k}</Text>
					</Pressable>
				))}
			</View>
		</ScrollView>
	);
}
