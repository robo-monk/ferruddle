// Planning, integrity, and statistics tests; never invoke an extractor or timer.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { digest, pairedPasses, selectInputs, hashFiles, assertStableHashes,
	validateRun, combineRuns, summarizeMode, compareMode, median, percentile, sampleStatistics } from './benchmark.shared.mjs';

const proof = { sha256: 'a'.repeat(64), contentBytes: 20, markdownBytes: 0, wordCount: 3 };
function example(pass = 0) {
	const modeOrder = pairedPasses()[pass].modeOrder;
	const selection = { schemaVersion: 2, kind: 'warm', pass, rounds: 3, warmups: 1,
		caseIds: ['one', 'two'], manifestSha256: 'manifest', inputSetSha256: 'inputs', modeOrder };
	const modes = modeOrder.map(mode => {
		const rawRounds = Array.from({ length: 4 }, (_, round) => ({ round, phase: round ? 'measured' : 'warmup',
			orderStart: round % 2, wallMs: 5 + round, caseMs: [1 + round, 2 + round] }));
		return { mode, roundTotalMs: rawRounds.slice(1).map(round => round.wallMs), rawRounds,
			perCase: selection.caseIds.map((id, index) => ({ id, ms: rawRounds.slice(1).map(round => round.caseMs[index]), output: proof })) };
	});
	return { selection, result: { ...selection, implementation: 'rust', caseCount: 2, modes } };
}

test('paired passes reverse both implementation order and mode order', () => {
	const passes = pairedPasses();
	assert.deepEqual(passes[0].implementations, ['rust', 'typescript']);
	assert.deepEqual(passes[1].implementations, ['typescript', 'rust']);
	assert.deepEqual(passes[1].modeOrder, [...passes[0].modeOrder].reverse());
});

test('selection hashes exactly the pinned HTML and URL and rejects changed inputs/exclusions', () => {
	const cases = ['one', 'two', 'math'].map(id => ({ id, url: `https://example.test/${id}`,
		inputHtml: `<article>${id}</article>`, inputSha256: digest(`<article>${id}</article>`) }));
	const files = new Map(cases.map(item => [`${item.id}.json`, Buffer.from(JSON.stringify(item))]));
	const manifest = Buffer.from(JSON.stringify({ caseCount: 3,
		cases: [...files].map(([file, bytes]) => ({ id: JSON.parse(bytes).id, file, sha256: digest(bytes) })) }));
	const math = Buffer.from(JSON.stringify({ math: {} }));
	const selected = selectInputs(manifest, math, file => files.get(file), 2);
	assert.deepEqual(selected.caseIds, ['one', 'two']);
	assert.equal(selected.inputs[0].url, cases[0].url);
	assert.equal(selected.inputSetSha256, digest(JSON.stringify(cases.slice(0, 2).map(({ id, url, inputSha256 }) => ({ id, url, inputSha256 })))));
	assert.throws(() => selectInputs(manifest, math, () => Buffer.from('changed'), 2), /Changed case/);
	assert.throws(() => selectInputs(manifest, Buffer.from('{"unknown":{}}'), file => files.get(file), 2), /Unknown/);
	assert.throws(() => selectInputs(manifest, math, file => files.get(file), 3), /Expected exactly/);
});

test('source provenance rejects changed, missing, and newly added files', () => {
	const directory = mkdtempSync(join(tmpdir(), 'ferruddle-benchmark-provenance-'));
	try {
		const file = join(directory, 'worker.mjs');
		writeFileSync(file, 'original');
		const before = hashFiles(directory, [file]);
		assertStableHashes(before, hashFiles(directory, [file]));
		writeFileSync(file, 'changed');
		assert.throws(() => assertStableHashes(before, hashFiles(directory, [file])), /worker.mjs/);
		assert.throws(() => assertStableHashes(before, {}), /worker.mjs/);
		assert.throws(() => assertStableHashes(before, { ...before, 'new.mjs': 'new' }), /new.mjs/);
	} finally { rmSync(directory, { recursive: true, force: true }); }
});

test('run validation checks identity, every sample dimension, mode order, and raw sample agreement', () => {
	const { selection, result } = example();
	validateRun(result, selection, 'rust');
	for (const mutate of [
		value => { value.inputSetSha256 = 'different'; },
		value => { value.modes.reverse(); },
		value => { value.modes[0].perCase[0].id = 'different'; },
		value => { value.modes[0].perCase[0].ms.pop(); },
		value => { value.modes[0].rawRounds[1].caseMs[0] = 99; },
		value => { value.modes[0].perCase[0].output.sha256 = 'missing'; },
		value => { value.modes[0].roundTotalMs[0] = Number.NaN; },
	]) {
		const changed = structuredClone(result); mutate(changed);
		assert.throws(() => validateRun(changed, selection, 'rust'));
	}
});

test('combined runs retain all measured and warmup samples and require stable outputs', () => {
	const first = example(0), second = example(1);
	const result = combineRuns([first.result, second.result]);
	assert.equal(result.measuredRoundsPerMode, 6);
	for (const mode of result.modes) {
		assert.equal(mode.perCase[0].ms.length, 6);
		assert.equal(mode.rawRounds.length, 8);
		assert.deepEqual(mode.rawRounds.map(round => round.pass), [0, 0, 0, 0, 1, 1, 1, 1]);
	}
	const changed = structuredClone(second.result); changed.modes[0].perCase[0].output.sha256 = 'b'.repeat(64);
	assert.throws(() => combineRuns([first.result, changed]), /output changed between passes/);
});

test('descriptive statistics expose slower individual cases even when aggregate is faster', () => {
	// These arbitrary numbers verify arithmetic/report dimensions, not performance.
	assert.equal(median([1, 9, 3, 5]), 4);
	assert.equal(percentile([1, 2, 3, 4], 0.95), 4);
	const { result } = example();
	const summary = summarizeMode(result.modes[0], 1024);
	assert.equal(summary.perCase.length, 2);
	assert.equal(summary.perCase[0].maxMs, 4);
	assert.equal(summary.perCase[0].upperQuartileMs, 4);
	assert.equal('p95Ms' in summary.perCase[0], false);
	assert.equal('corpusP95Ms' in summary, false);
	const native = { corpusMedianMs: 2, perCase: [{ id: 'slow', medianMs: 2, p95Ms: 3 }, { id: 'fast', medianMs: 1, p95Ms: 2 }] };
	const reference = { corpusMedianMs: 10, perCase: [{ id: 'slow', medianMs: 1, p95Ms: 2 }, { id: 'fast', medianMs: 9, p95Ms: 10 }] };
	const comparison = compareMode(native, reference);
	assert.equal(comparison.speedup, 5);
	assert.deepEqual(comparison.rustSlowerCases.map(sample => sample.id), ['slow']);
});

test('small samples name the maximum and reserve p95 for at least 20 samples', () => {
	const small = sampleStatistics([...Array(13).fill(10), 90]);
	assert.equal(small.sampleCount, 14);
	assert.equal(small.maxMs, 90);
	assert.equal(small.upperQuartileMs, 10);
	assert.equal('p95Ms' in small, false);
	const larger = sampleStatistics(Array.from({ length: 20 }, (_, i) => i + 1));
	assert.equal(larger.p95Ms, 19);
	assert.equal(larger.maxMs, 20);
});
