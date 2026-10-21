import { defineConfig, mergeConfig } from 'vitest/config';
import baseConfig from '../../vitest.config';

// Explicitly invoked; *.oracle.ts files are outside Vitest's default discovery.
export default mergeConfig(baseConfig, defineConfig({
	test: {
		include: [process.env.DEFUDDLE_RUST_RESULTS
			? 'scripts/porting/compare.oracle.ts'
			: 'scripts/porting/export.oracle.ts'],
		maxWorkers: 1,
		testTimeout: 120_000,
	},
}));
