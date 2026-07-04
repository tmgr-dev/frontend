import dynamicMask from './dynamic-mask';
import maskit from './maskit';
// Facade to maskit/dynamicMask when mask is String or Array
export default function (value, mask, masked = true, tokens) {
	return Array.isArray(mask)
		? dynamicMask(maskit, mask, tokens)(value, mask, masked, tokens)
		: maskit(value, mask, masked, tokens);
}
