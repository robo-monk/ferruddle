// Repeat the original and current native runtimes with the same public-API worker.
import { execFileSync } from 'node:child_process';
import { copyFileSync, chmodSync, constants, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { MODES, digest, hashFiles, sourceFiles, assertStableHashes, validateRun,
	combineRuns, summarizeMode, percentile, median } from './benchmark.shared.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const [outputArg, baselineExecutableArg, baselineCrateArg] = process.argv.slice(2);
if (!outputArg || !baselineExecutableArg || !baselineCrateArg) {
	throw new Error('Usage: node scripts/porting/benchmark-native-repeat.mjs MAIN_OUTPUT BASELINE_EXECUTABLE BASELINE_CRATE');
}
const output = resolve(outputArg), baselineBuiltExecutable = resolve(baselineExecutableArg), baselineCrate = resolve(baselineCrateArg);
const baselineExecutable = join(output, 'bin/defuddle-benchmark-baseline');
const read = file => JSON.parse(readFileSync(file, 'utf8'));
const summary = read(join(output, 'summary.json'));
if (summary.schemaVersion !== 2 || summary.caseCount !== 208 || summary.passes !== 2) {
	throw new Error('Expected a completed schema-2 paired benchmark with all 208 pages.');
}
const currentExecutable = summary.nativeExecutable;
const baselineContract = read(join(root, 'rust/docs/benchmarks/baseline/contract.json'));
const originalHashes = baselineContract.runtimeSourceHashes;
const baselinePaths = () => [...sourceFiles(join(baselineCrate, 'src')), join(baselineCrate, 'Cargo.toml'), join(baselineCrate, 'Cargo.lock')];
const binaryHashes = { baseline: digest(readFileSync(baselineBuiltExecutable)), current: digest(readFileSync(currentExecutable)) };
if (binaryHashes.current !== summary.executableSha256) throw new Error('Current executable changed since the paired run.');
function stable() {
	assertStableHashes(originalHashes, hashFiles(baselineCrate, baselinePaths()));
	for (const group of ['rustSourceHashes', 'referenceAndHarnessHashes', 'inputHashes']) {
		if (!summary[group]) throw new Error(`Missing paired provenance: ${group}`);
		const files = Object.keys(summary[group]).map(file => join(root, file));
		if (group === 'rustSourceHashes') files.push(...sourceFiles(join(root, 'rust/src')));
		if (group === 'referenceAndHarnessHashes') files.push(...sourceFiles(join(root, 'src')), ...sourceFiles(join(root, 'scripts/porting')));
		assertStableHashes(summary[group], hashFiles(root, files));
	}
	for (const [name, executable] of Object.entries({ baseline: baselineExecutable, current: currentExecutable })) {
		if (digest(readFileSync(executable)) !== binaryHashes[name]) throw new Error(`${name} executable changed.`);
	}
	for (const file of ['examples/benchmark.rs', 'tests/data/quality/math.json']) {
		if (digest(readFileSync(join(baselineCrate, file))) !== digest(readFileSync(join(root, 'rust', file)))) {
			throw new Error(`Baseline must use the identical current worker/selection contract: ${file}`);
		}
	}
}
const selections = [1, 2].map(pass => read(join(output, `rust-pass-${pass}-selection.json`)));
for (const [index, selection] of selections.entries()) {
	if (selection.pass !== index || selection.kind !== 'warm' || selection.caseIds.length !== 208
		|| selection.inputSetSha256 !== summary.inputSetSha256 || selection.manifestSha256 !== summary.manifestSha256
		|| selection.rounds !== summary.rounds || selection.warmups !== summary.warmups) {
		throw new Error('Repeat selections do not match the completed paired run.');
	}
}
const schedule = [['baseline', 0], ['current', 0], ['current', 1], ['baseline', 1]];
const reportPath = join(output, 'native-repeat-summary.json');
const rawPaths = schedule.map(([name, pass]) => join(output, `native-repeat-${name}-${pass + 1}.json`));
if ([reportPath, ...rawPaths].some(existsSync)) throw new Error('Refusing to overwrite native repeat evidence.');
copyFileSync(baselineBuiltExecutable, baselineExecutable, constants.COPYFILE_EXCL);
chmodSync(baselineExecutable, 0o755);
stable();
const runs = { baseline: [], current: [] }, executionOrder = [];
for (const [index, [name, pass]] of schedule.entries()) {
	stable();
	const startedAt = new Date().toISOString(), loadBefore = os.loadavg();
	execFileSync(name === 'baseline' ? baselineExecutable : currentExecutable,
		[join(output, `rust-pass-${pass + 1}-selection.json`), rawPaths[index]], { cwd: root, stdio: 'inherit', env: { ...process.env, TZ: 'UTC' } });
	executionOrder.push({ name, pass, startedAt, finishedAt: new Date().toISOString(), loadBefore, loadAfter: os.loadavg() });
	stable();
	runs[name][pass] = validateRun(read(rawPaths[index]), selections[pass], 'rust');
}
const combined = Object.fromEntries(Object.entries(runs).map(([name, values]) => [name, combineRuns(values)]));
function comparison(current, baseline) {
	const perCase = current.perCase.map((item, index) => {
		const other = baseline.perCase[index];
		if (item.id !== other.id) throw new Error('Case order differs.');
		return { id: item.id, currentMedianMs: item.medianMs, baselineMedianMs: other.medianMs,
			currentUpperQuartileMs: item.upperQuartileMs, baselineUpperQuartileMs: other.upperQuartileMs,
			currentMaxMs: item.maxMs, baselineMaxMs: other.maxMs,
			currentToBaselineRatio: item.medianMs / other.medianMs,
			deltaMs: item.medianMs - other.medianMs };
	});
	return { currentToBaselineRatio: current.corpusMedianMs / baseline.corpusMedianMs, perCase };
}
const prior = baselineContract.regressionWatchlist;
const modes = MODES.map(mode => {
	const find = run => run.modes.find(item => item.mode === mode);
	const current = summarizeMode(find(combined.current), summary.inputBytes);
	const baseline = summarizeMode(find(combined.baseline), summary.inputBytes);
	const compared = comparison(current, baseline);
	const perPass = [0, 1].map(pass => {
		const current = summarizeMode(find(runs.current[pass]), summary.inputBytes);
		const baseline = summarizeMode(find(runs.baseline[pass]), summary.inputBytes);
		return { pass, current, baseline, ...comparison(current, baseline) };
	});
	const cases = compared.perCase.map(item => ({ ...item,
		passRatios: perPass.map(pass => pass.perCase.find(other => other.id === item.id).currentToBaselineRatio) }));
	const aaCaseRatios = perPass[0].current.perCase.map((item, index) => {
		const ratio = perPass[1].current.perCase[index].medianMs / item.medianMs;
		return { id: item.id, pass2ToPass1Ratio: ratio, foldDifference: Math.max(ratio, 1 / ratio) };
	});
	const aaNoise = { basis: 'Fold differences between current-runtime pass medians on all 208 pages; descriptive, not a confidence interval.',
		medianFoldDifference: median(aaCaseRatios.map(item => item.foldDifference)),
		p95FoldDifference: percentile(aaCaseRatios.map(item => item.foldDifference), 0.95), perCase: aaCaseRatios };
	const regressionFlagThreshold = Math.max(1.05, aaNoise.p95FoldDifference);
	return { mode, current, baseline, ...compared, perCase: cases, perPass,
		aaNoise, regressionFlagThreshold,
		slowerCases: cases.filter(item => item.currentToBaselineRatio > 1),
		slowerOver5PercentInBothPasses: cases.filter(item => item.passRatios.every(ratio => ratio > 1.05)),
		slowerBeyondNoiseFloorInBothPasses: cases.filter(item => item.passRatios.every(ratio => ratio > regressionFlagThreshold)),
		priorFlaggedCases: prior.filter(item => item.mode === mode).map(item => cases.find(other => other.id === item.case)),
		schemaLazyLookupCase: cases.find(item => item.id === 'general--github.com-issue-56'),
		changedOutputProofs: find(combined.current).perCase.filter((item, index) =>
			JSON.stringify(item.output) !== JSON.stringify(find(combined.baseline).perCase[index].output)).map(item => item.id) };
});
const report = { schemaVersion: 2, date: new Date().toISOString(), caseCount: summary.caseCount,
	inputSetSha256: summary.inputSetSha256, manifestSha256: summary.manifestSha256,
	measuredRoundsPerMode: summary.measuredRoundsPerMode, environment: summary.environment,
	baselineBuiltExecutable, baselineExecutable, currentExecutable, binaryHashes, baselineSourceHashes: originalHashes,
	workerSha256: digest(readFileSync(join(root, 'rust/examples/benchmark.rs'))), executionOrder,
	measurement: 'Same schema-2 worker and complete 208-page corpus for both runtimes. Sequential baseline/current/current/baseline, with reversed mode order in pass 2. Full output validation is outside timers. Output differences reflect quality fixes; equal outputs are not assumed. Ratios and 5% flags are descriptive, not statistical significance tests.',
	modes };
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
for (const mode of modes) console.log(`${mode.mode}: current/baseline ${mode.currentToBaselineRatio.toFixed(3)}; ${mode.slowerOver5PercentInBothPasses.length} pages >5% slower in both passes`);
