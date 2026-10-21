// Paired Rust/TypeScript measurements over identical pinned HTML strings/URLs.
import { execFileSync } from 'node:child_process';
import { copyFileSync, chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { MODES, IMPLEMENTATIONS, digest, pairedPasses, selectInputs, sourceFiles,
	hashFiles, assertStableHashes, validateRun, combineRuns, summarizeMode, compareMode, sampleStatistics } from './benchmark.shared.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const validateOnly = process.argv[2] === '--validate';
const output = resolve(validateOnly ? join(root, '.benchmarks/results/latest') : (process.argv[2] ?? join(root, '.benchmarks/results/latest')));
const rounds = Number(process.argv[3] ?? 7), warmups = Number(process.argv[4] ?? 2);
if (!Number.isSafeInteger(rounds) || rounds < 3 || !Number.isSafeInteger(warmups) || warmups < 1) {
	throw new Error('Usage: node tools/benchmark/benchmark.mjs [OUTPUT_DIR] [ROUNDS>=3] [WARMUPS>=1], or --validate');
}
const directory = join(root, 'tests/data/upstream');
const manifestPath = join(directory, 'manifest.json');
const mathPath = join(root, 'tests/data/quality/math.json');
const selection = { ...selectInputs(readFileSync(manifestPath), readFileSync(mathPath), file => readFileSync(join(directory, file))), rounds, warmups };
if (!selection.caseIds.some(id => id.includes('lesswrong'))) throw new Error('LessWrong must remain in the headline corpus.');
if (validateOnly) {
	console.log(JSON.stringify({ valid: true, caseCount: selection.caseIds.length, inputBytes: selection.inputBytes,
		manifestSha256: selection.manifestSha256, inputSetSha256: selection.inputSetSha256, passes: pairedPasses(),
		coldCaseId: 'issues--132-hero-class', extractionCalls: 0 }, null, 2));
	process.exit(0);
}
const referenceRoot = resolve(process.env.FERRUDDLE_REFERENCE_DIR ?? join(root, '.benchmarks/reference'));
const referenceMetadataPath = join(referenceRoot, 'ferruddle-reference.json');
if (!existsSync(referenceMetadataPath) || !existsSync(join(referenceRoot, 'node_modules/vitest/vitest.mjs'))) {
	throw new Error('Prepare the pinned reference first: node tools/benchmark/prepare-reference.mjs');
}
const referenceMetadata = JSON.parse(readFileSync(referenceMetadataPath, 'utf8'));
const archivedSource = JSON.parse(readFileSync(join(root, 'docs/benchmarks/measured-source.json'), 'utf8'));
const expectedReference = Object.fromEntries(Object.entries(archivedSource.files).filter(([file]) => file !== 'node_modules/.package-lock.json'));
if (referenceMetadata.archiveSha256 !== archivedSource.archiveSha256) throw new Error('Wrong TypeScript reference archive.');
assertStableHashes(expectedReference, referenceMetadata.files);
assertStableHashes(expectedReference, hashFiles(referenceRoot, Object.keys(expectedReference).map(file => join(referenceRoot, file))));

// Never overwrite saved benchmark evidence.
if (existsSync(output) && readdirSync(output).length) throw new Error(`Use a new empty output directory: ${output}`);
mkdirSync(output, { recursive: true });
const write = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n');
write(join(output, 'selection.json'), selection);
const cargo = process.env.CARGO ?? 'cargo', rustc = process.env.RUSTC ?? 'rustc', rustdoc = process.env.RUSTDOC ?? 'rustdoc';
function configurationPaths() {
	const paths = [];
	for (let directory = root; ; directory = dirname(directory)) {
		paths.push(...['.cargo/config', '.cargo/config.toml', 'rust-toolchain', 'rust-toolchain.toml'].map(file => join(directory, file)));
		if (dirname(directory) === directory) break;
	}
	const cargoHome = process.env.CARGO_HOME ?? join(os.homedir(), '.cargo');
	paths.push(join(cargoHome, 'config'), join(cargoHome, 'config.toml'));
	return paths.filter(existsSync);
}
const rustPaths = () => [...sourceFiles(join(root, 'src')), join(root, 'Cargo.toml'), join(root, 'Cargo.lock')];
const referencePaths = () => [...sourceFiles(join(root, 'tools/benchmark')), ...configurationPaths(),
	join(root, 'examples/benchmark.rs'), join(root, 'docs/benchmarks/methodology.md'),
	join(root, 'docs/benchmarks/measured-source.json'), join(root, 'docs/benchmarks/measured-source.tar.gz')];
const pinnedReferencePaths = () => [...Object.keys(expectedReference).map(file => join(referenceRoot, file)),
	join(referenceRoot, 'node_modules/.package-lock.json'), referenceMetadataPath];
const inputPaths = [manifestPath, mathPath, ...selection.inputs.map(input => join(directory, input.file))];
const snapshot = () => ({ rustSourceHashes: hashFiles(root, rustPaths()),
	referenceAndHarnessHashes: hashFiles(root, referencePaths()), referenceSourceHashes: hashFiles(referenceRoot, pinnedReferencePaths()), inputHashes: hashFiles(root, inputPaths) });
const provenance = snapshot();
const executableCopies = new Map();
const stable = () => {
	const current = snapshot();
	for (const group of Object.keys(provenance)) assertStableHashes(provenance[group], current[group]);
	for (const [path, hash] of executableCopies) {
		if (digest(readFileSync(path)) !== hash) throw new Error(`Benchmark executable changed: ${path}`);
	}
};
const version = command => execFileSync(command, ['--version'], { encoding: 'utf8' }).trim();
const dependencyVersions = Object.fromEntries(['linkedom', 'turndown', 'vitest', 'vite', 'typescript', 'temml']
	.map(name => [name, JSON.parse(readFileSync(join(referenceRoot, 'node_modules', name, 'package.json'), 'utf8')).version]));
const environment = { platform: os.platform(), architecture: os.arch(), kernel: os.release(), cpu: os.cpus()[0].model,
	logicalCpus: os.cpus().length, availableParallelism: os.availableParallelism(), totalMemoryBytes: os.totalmem(),
	node: process.version, nodeVersions: process.versions, nodeExecutable: process.execPath, cargo: version(cargo),
	rustc: version(rustc), rustcVerbose: execFileSync(rustc, ['-vV'], { encoding: 'utf8' }).trim(),
	rustdoc: version(rustdoc), commands: { cargo, rustc, rustdoc }, dependencyVersions,
	cpuFrequencyGovernor: existsSync('/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor')
		? readFileSync('/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor', 'utf8').trim() : null,
	// Hash overrides instead of publishing arbitrary environment values or URLs.
	buildEnvironmentValueHashes: Object.fromEntries(Object.entries(process.env)
		.filter(([name]) => /^(CARGO_|RUST)/.test(name) && !/(TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTH|PRIVATE|KEY)/i.test(name))
		.sort(([left], [right]) => left.localeCompare(right)).map(([name, value]) => [name, digest(value)])),
	settings: { timezone: 'UTC', dom: 'linkedom', cargoTargetDir: process.env.CARGO_TARGET_DIR ?? null,
		cargoBuildJobs: process.env.CARGO_BUILD_JOBS ?? null, cargoIncremental: process.env.CARGO_INCREMENTAL ?? null,
		rustflags: process.env.RUSTFLAGS ?? '', nodeOptions: process.env.NODE_OPTIONS ?? '' } };
write(join(output, 'provenance.json'), { ...provenance, environment, environmentSha256: digest(JSON.stringify(environment)) });
const artifacts = execFileSync(cargo, ['build', '--release', '--locked', '--manifest-path', 'Cargo.toml',
	'--example', 'benchmark', '--message-format=json-render-diagnostics'], {
	cwd: root, encoding: 'utf8', stdio: ['inherit', 'pipe', 'inherit'], maxBuffer: 16 * 1024 * 1024,
}).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
const builtExecutable = artifacts.findLast(message => message.reason === 'compiler-artifact'
	&& message.target.name === 'benchmark' && message.target.kind.includes('example') && message.executable)?.executable;
if (!builtExecutable) throw new Error('Cargo did not identify the benchmark executable.');
mkdirSync(join(output, 'bin'));
const executable = join(output, 'bin/ferruddle-benchmark');
copyFileSync(builtExecutable, executable);
chmodSync(executable, 0o755);
const executableSha256 = digest(readFileSync(executable));
executableCopies.set(executable, executableSha256);
stable();
const executionOrder = [];
function runWorker(implementation, config, name) {
	stable();
	const selectionPath = join(output, `${name}-selection.json`), resultPath = join(output, `${name}.json`);
	write(selectionPath, config);
	const startedAt = new Date().toISOString(), loadBefore = os.loadavg();
	const program = implementation === 'rust' ? executable : process.execPath;
	const args = implementation === 'rust' ? [selectionPath, resultPath]
		: ['node_modules/vitest/vitest.mjs', 'run', '--config', 'scripts/porting/benchmark.config.ts'];
	execFileSync(program, args, { cwd: implementation === 'rust' ? root : referenceRoot, stdio: 'inherit', env: { ...process.env, TZ: 'UTC', DOM: 'linkedom',
		DEFUDDLE_BENCH_SELECTION: selectionPath, DEFUDDLE_BENCH_OUTPUT: resultPath } });
	executionOrder.push({ name, implementation, kind: config.kind, pass: config.pass, modeOrder: config.modeOrder,
		startedAt, finishedAt: new Date().toISOString(), loadBefore, loadAfter: os.loadavg() });
	stable();
	return validateRun(JSON.parse(readFileSync(resultPath, 'utf8')), config, implementation);
}

// Independent processes per pass. Reverse both implementation and mode order.
// No tests, builds, workers or output walks run concurrently with these timers.
const runs = Object.fromEntries(IMPLEMENTATIONS.map(implementation => [implementation, []]));
for (const pass of pairedPasses()) {
	const config = { ...selection, kind: 'warm', pass: pass.pass, modeOrder: pass.modeOrder };
	for (const implementation of pass.implementations) {
		runs[implementation].push(runWorker(implementation, config, `${implementation}-pass-${pass.pass + 1}`));
	}
}
const combined = Object.fromEntries(IMPLEMENTATIONS.map(implementation => [implementation, combineRuns(runs[implementation])]));
for (const implementation of IMPLEMENTATIONS) write(join(output, `${implementation}.json`), combined[implementation]);

// Same pinned 319-byte page used by the performance review. Every sample is
// the first extraction in a fresh process, after module import and input IO.
const coldId = 'issues--132-hero-class';
const coldInput = selection.inputs.find(input => input.id === coldId);
if (!coldInput) throw new Error(`Missing cold input ${coldId}`);
const coldBytes = Buffer.byteLength(JSON.parse(readFileSync(join(directory, coldInput.file), 'utf8')).inputHtml);
const coldIdentity = [{ id: coldInput.id, url: coldInput.url, inputSha256: coldInput.inputSha256 }];
const coldSamples = [];
for (let sample = 0; sample < rounds; sample++) {
	const modes = sample % 2 ? [...MODES].reverse() : MODES;
	const implementations = sample % 2 ? [...IMPLEMENTATIONS].reverse() : IMPLEMENTATIONS;
	for (const mode of modes) {
		for (const implementation of implementations) {
			const config = { ...selection, kind: 'cold', pass: sample, passes: 1, rounds: 1, warmups: 0,
				caseIds: [coldId], inputs: [coldInput], inputBytes: coldBytes,
				inputSetSha256: digest(JSON.stringify(coldIdentity)), modeOrder: [mode] };
			const name = `${implementation}-cold-${sample + 1}-${mode === 'html' ? 'html' : 'markdown'}`;
			const result = runWorker(implementation, config, name), measured = result.modes[0];
			const warmProof = combined[implementation].modes.find(item => item.mode === mode).perCase
				.find(item => item.id === coldId).output;
			if (JSON.stringify(measured.perCase[0].output) !== JSON.stringify(warmProof)) {
				throw new Error(`Cold/warm output differs for ${implementation} ${mode}`);
			}
			coldSamples.push({ implementation, mode, sample, ms: measured.roundTotalMs[0],
				extractionMs: measured.perCase[0].ms[0], output: measured.perCase[0].output, rawFile: `${name}.json` });
		}
	}
}
const cold = { case: { ...coldInput, inputBytes: coldBytes }, samplesPerImplementationMode: rounds,
	measurement: 'First extraction/DOM parse in a fresh process after imports and pinned input IO. No warmup. Process startup, module evaluation and Vitest transformation are excluded; cold API cost is not process launch latency.',
	samples: coldSamples, modes: MODES.map(mode => ({ mode, ...Object.fromEntries(IMPLEMENTATIONS.map(implementation => {
		const ms = coldSamples.filter(sample => sample.mode === mode && sample.implementation === implementation).map(sample => sample.extractionMs);
		return [implementation, { ms, ...sampleStatistics(ms) }];
	})) })) };
write(join(output, 'cold.json'), cold);
stable();
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const report = {
	schemaVersion: 2, date: new Date().toISOString(), upstreamRevision: manifest.upstreamRevision, upstreamVersion: manifest.upstreamVersion,
	manifestSha256: selection.manifestSha256, inputSetSha256: selection.inputSetSha256,
	caseCount: selection.caseIds.length, excludedMathCaseCount: selection.excludedMathCaseIds.length, inputBytes: selection.inputBytes,
	rounds, warmups, passes: 2, measuredRoundsPerMode: rounds * 2, environment,
	environmentSha256: digest(JSON.stringify(environment)), ...provenance, nativeBuiltExecutable: builtExecutable,
	nativeExecutable: executable, executableSha256, executionOrder,
	statistics: 'Nearest-rank upper quartile, explicit minimum/maximum and sample count. p95 is emitted only for at least 20 samples; with 7 or 14 samples the maximum is not labeled tail latency. Page p95 is across 208 page medians.',
	measurement: 'Two sequential passes in fresh implementation processes. Pass 1 Rust then TypeScript, HTML then HTML+Markdown; pass 2 TypeScript then Rust, HTML+Markdown then HTML. Every case uses the same pinned input string/URL with a fresh DOM; order rotates by round. Full-corpus wall timers include parse, extraction, metadata and HTML serialization, optionally Markdown, plus minimal timing/collection bookkeeping. Per-case timers exclude that bookkeeping. Imports/build/input IO/output validation/hash walks and result disposal are outside all warm timers. Two whole-corpus warmups and seven measured rounds per mode/pass by default. Rust release thin LTO; TypeScript Node/linkedom useAsync:false. All 208 non-math cases, including LessWrong, remain in aggregate results.',
	referenceDirectory: referenceRoot,
	referenceRuntime: 'Archived TypeScript source under Vitest/Vite SSR and linkedom, rather than the distributed dist/node.js bundle.',
	garbageCollection: 'Rust DOM destruction inside extract is timed. V8 may reclaim DOM garbage during API work or during untimed validation allocations. No forced GC is used.',
	outputValidation: 'Every warmup/measured output is nonempty and fingerprinted after timers; own output must stay identical across rounds, paired passes, and cold/warm calls. TypeScript checks all projected fields against the pinned oracle; native rejects math warnings and incorrect Markdown options. Native semantic correctness is established by the separate corpus gate, not benchmark fingerprints.',
	cold: { caseId: coldId, inputBytes: coldBytes, samplesPerImplementationMode: rounds, file: 'cold.json', measurement: cold.measurement },
	modes: MODES.map(modeName => {
		const native = summarizeMode(combined.rust.modes.find(mode => mode.mode === modeName), selection.inputBytes);
		const reference = summarizeMode(combined.typescript.modes.find(mode => mode.mode === modeName), selection.inputBytes);
		const comparison = compareMode(native, reference);
		const perPass = pairedPasses().map(pass => {
			const native = summarizeMode(runs.rust[pass.pass].modes.find(mode => mode.mode === modeName), selection.inputBytes);
			const reference = summarizeMode(runs.typescript[pass.pass].modes.find(mode => mode.mode === modeName), selection.inputBytes);
			return { pass: pass.pass, rust: native, typescript: reference, ...compareMode(native, reference) };
		});
		return { mode: modeName, rust: native, typescript: reference, ...comparison, perPass,
			rustSlowerInAnyPass: comparison.perCase.filter((_, index) => perPass.some(pass => pass.perCase[index].speedup < 1))
				.map(sample => ({ ...sample, passSpeedups: perPass.map(pass => pass.perCase.find(item => item.id === sample.id).speedup) })) };
	}),
};
write(join(output, 'summary.json'), report);
console.log(report.modes.map(mode => `${mode.mode}: Rust ${mode.rust.corpusMedianMs.toFixed(1)} ms; TS ${mode.typescript.corpusMedianMs.toFixed(1)} ms; ${mode.speedup.toFixed(2)}x; ${mode.rustSlowerCases.length} cases slower by combined median (${mode.rustSlowerInAnyPass.length} in either pass)`).join('\n'));
