import variant from '@jitl/quickjs-wasmfile-release-sync';
import wasmUrl from '@jitl/quickjs-wasmfile-release-sync/wasm?url';
import {
	newQuickJSWASMModuleFromVariant,
	newVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';
import type { Endpoint, FromWorker, ToWorker } from './protocol';
import { runPluginWorker } from './workerRuntime';

let quickjs: Promise<QuickJSWASMModule> | null = null;

runPluginWorker(
	self as unknown as Endpoint<ToWorker, FromWorker>,
	() =>
		(quickjs ??= newQuickJSWASMModuleFromVariant(
			newVariant(variant, { wasmLocation: wasmUrl }),
		)),
);
