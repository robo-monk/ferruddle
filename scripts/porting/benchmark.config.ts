import { defineConfig, mergeConfig } from 'vitest/config';
import baseConfig from '../../vitest.config';

export default mergeConfig(baseConfig, defineConfig({
	test: { include: ['scripts/porting/benchmark.oracle.ts'], maxWorkers: 1, testTimeout: 600_000 },
}));
