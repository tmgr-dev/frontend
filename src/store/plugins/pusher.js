import { getSharedEcho } from '@/composable/usePusher';

// Compatibility factory: all consumers share the composable transport.
export default function () {
	return getSharedEcho();
}
