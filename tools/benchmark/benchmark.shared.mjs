// Pure benchmark planning, provenance, validation, and descriptive statistics.
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';

export const MODES = Object.freeze(['html', 'html+markdown']);
export const IMPLEMENTATIONS = Object.freeze(['rust', 'typescript']);
export const digest = bytes => createHash('sha256').update(bytes).digest('hex');

export function pairedPasses() {
	return [
		{ pass: 0, implementations: [...IMPLEMENTATIONS], modeOrder: [...MODES] },
		{ pass: 1, implementations: [...IMPLEMENTATIONS].reverse(), modeOrder: [...MODES].reverse() },
	];
}

export function selectInputs(manifestBytes, mathBytes, loadCase, expectedCount = 208) {
	const manifest = JSON.parse(manifestBytes);
	const exclusions = Object.keys(JSON.parse(mathBytes));
	const allIds = manifest.cases.map(entry => entry.id);
	if (new Set(allIds).size !== allIds.length || manifest.caseCount !== allIds.length
		|| exclusions.some(id => !allIds.includes(id))) throw new Error('Unknown or duplicate corpus IDs/exclusions.');
	const mathIds = new Set(exclusions);
	let inputBytes = 0;
	const inputs = manifest.cases.filter(entry => !mathIds.has(entry.id)).map(entry => {
		const bytes = loadCase(entry.file);
		if (digest(bytes) !== entry.sha256) throw new Error(`Changed case ${entry.id}`);
		const item = JSON.parse(bytes);
		if (item.id !== entry.id || typeof item.url !== 'string' || !item.url
			|| typeof item.inputHtml !== 'string' || digest(item.inputHtml) !== item.inputSha256) {
			throw new Error(`Changed input or URL for ${entry.id}`);
		}
		inputBytes += Buffer.byteLength(item.inputHtml);
		return { id: item.id, file: entry.file, sha256: entry.sha256, inputSha256: item.inputSha256, url: item.url };
	});
	if (inputs.length !== expectedCount || inputs.length + exclusions.length !== manifest.caseCount) {
		throw new Error(`Expected exactly ${expectedCount} non-math cases; got ${inputs.length}.`);
	}
	return { schemaVersion: 2, kind: 'warm', passes: 2, manifestSha256: digest(manifestBytes),
		mathExclusionsSha256: digest(mathBytes), caseIds: inputs.map(item => item.id), inputs,
		excludedMathCaseIds: exclusions.sort(), inputBytes,
		inputSetSha256: digest(JSON.stringify(inputs.map(({ id, url, inputSha256 }) => ({ id, url, inputSha256 })))) };
}

export function sourceFiles(directory) {
	return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
		const path = join(directory, entry.name);
		return entry.isDirectory() ? sourceFiles(path) : [path];
	});
}
export function hashFiles(root, files) {
	return Object.fromEntries([...new Set(files)].sort().map(path => [relative(root, path), digest(readFileSync(path))]));
}
export function assertStableHashes(expected, actual) {
	const changed = [...new Set([...Object.keys(expected), ...Object.keys(actual)])]
		.filter(path => expected[path] !== actual[path]);
	if (changed.length) throw new Error(`Sources or benchmark inputs changed during measurement: ${changed.join(', ')}`);
}

const positive = value => Number.isFinite(value) && value > 0;
const equal = (left, right) => JSON.stringify(left) === JSON.stringify(right);
export function validateRun(result, selection, implementation) {
	const fail = message => { throw new Error(`${implementation}: ${message}`); };
	if (result.schemaVersion !== 2 || result.implementation !== implementation || result.kind !== selection.kind
		|| result.pass !== selection.pass || result.caseCount !== selection.caseIds.length
		|| result.rounds !== selection.rounds || result.warmups !== selection.warmups
		|| result.manifestSha256 !== selection.manifestSha256 || result.inputSetSha256 !== selection.inputSetSha256
		|| !equal(result.modeOrder, selection.modeOrder) || result.modes?.length !== selection.modeOrder.length) {
		fail('benchmark provenance or dimensions differ from selection.');
	}
	for (const [modeIndex, mode] of result.modes.entries()) {
		if (mode.mode !== selection.modeOrder[modeIndex] || mode.roundTotalMs?.length !== selection.rounds
			|| !mode.roundTotalMs.every(positive) || mode.perCase?.length !== selection.caseIds.length
			|| mode.rawRounds?.length !== selection.rounds + selection.warmups) fail('missing or invalid mode/round samples.');
		for (const [index, sample] of mode.perCase.entries()) {
			if (sample.id !== selection.caseIds[index] || sample.ms?.length !== selection.rounds
				|| !sample.ms.every(positive) || !/^[a-f0-9]{64}$/.test(sample.output?.sha256 ?? '')
				|| !Number.isSafeInteger(sample.output?.contentBytes) || sample.output.contentBytes <= 0
				|| !Number.isSafeInteger(sample.output?.markdownBytes) || sample.output.markdownBytes < 0
				|| !Number.isSafeInteger(sample.output?.wordCount) || sample.output.wordCount < 0) fail(`invalid case samples/output: ${sample.id}`);
		}
		for (const [index, round] of mode.rawRounds.entries()) {
			const measured = index >= selection.warmups;
			if (round.round !== index || round.phase !== (measured ? 'measured' : 'warmup')
				|| round.orderStart !== index % selection.caseIds.length || !positive(round.wallMs)
				|| round.caseMs?.length !== selection.caseIds.length || !round.caseMs.every(positive)) fail('invalid raw round/order.');
			if (measured && (round.wallMs !== mode.roundTotalMs[index - selection.warmups]
				|| round.caseMs.some((ms, caseIndex) => ms !== mode.perCase[caseIndex].ms[index - selection.warmups]))) {
				fail('raw samples do not agree with measured samples.');
			}
		}
	}
	return result;
}

export const median = values => {
	if (!values.length) throw new Error('Cannot summarize empty samples.');
	const sorted = [...values].sort((a, b) => a - b), mid = sorted.length >> 1;
	return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
export const percentile = (values, fraction) => {
	if (!values.length || fraction <= 0 || fraction > 1) throw new Error('Invalid percentile.');
	return [...values].sort((a, b) => a - b)[Math.ceil(values.length * fraction) - 1];
};

// With fewer than 20 samples nearest-rank p95 is just the maximum. Keep the
// maximum explicit and describe spread without implying measured tail latency.
export function sampleStatistics(values) {
	return { sampleCount: values.length, medianMs: median(values),
		upperQuartileMs: percentile(values, 0.75),
		...(values.length >= 20 ? { p95Ms: percentile(values, 0.95) } : {}),
		minMs: Math.min(...values), maxMs: Math.max(...values) };
}

export function combineRuns(runs) {
	if (runs.length !== 2 || runs[0].pass !== 0 || runs[1].pass !== 1) throw new Error('Expected both paired passes.');
	const first = runs[0];
	const modes = MODES.map(modeName => {
		const modes = runs.map(run => run.modes.find(mode => mode.mode === modeName));
		const perCase = modes[0].perCase.map((sample, index) => {
			if (!equal(sample.output, modes[1].perCase[index].output)) {
				throw new Error(`${first.implementation}: output changed between passes for ${sample.id} (${modeName}).`);
			}
			return { id: sample.id, ms: modes.flatMap(mode => mode.perCase[index].ms), output: sample.output };
		});
		return { mode: modeName, roundTotalMs: modes.flatMap(mode => mode.roundTotalMs), perCase,
			rawRounds: runs.flatMap((run, index) => modes[index].rawRounds.map(round => ({ pass: run.pass, ...round }))) };
	});
	return { schemaVersion: 2, implementation: first.implementation, kind: 'warm', caseCount: first.caseCount,
		rounds: first.rounds, warmups: first.warmups, passes: 2, measuredRoundsPerMode: first.rounds * 2,
		manifestSha256: first.manifestSha256, inputSetSha256: first.inputSetSha256, modes };
}

export function summarizeMode(mode, inputBytes) {
	const perCase = mode.perCase.map(sample => ({ id: sample.id, ...sampleStatistics(sample.ms) }));
	const measured = mode.rawRounds.filter(round => round.phase === 'measured');
	const extractionOnlyMs = measured.map(round => round.caseMs.reduce((sum, ms) => sum + ms, 0));
	return { corpusMedianMs: median(mode.roundTotalMs), corpusSampleCount: mode.roundTotalMs.length,
		corpusUpperQuartileMs: percentile(mode.roundTotalMs, 0.75),
		...(mode.roundTotalMs.length >= 20 ? { corpusP95Ms: percentile(mode.roundTotalMs, 0.95) } : {}),
		corpusMinMs: Math.min(...mode.roundTotalMs), corpusMaxMs: Math.max(...mode.roundTotalMs),
		extractionOnlyCorpusMedianMs: median(extractionOnlyMs),
		pageMedianMs: median(perCase.map(sample => sample.medianMs)),
		pageP95Ms: percentile(perCase.map(sample => sample.medianMs), 0.95),
		inputMiBPerSecond: inputBytes / (1024 * 1024) / (median(mode.roundTotalMs) / 1000), perCase };
}

export function compareMode(native, reference) {
	const perCase = native.perCase.map((sample, index) => {
		const other = reference.perCase[index];
		if (other?.id !== sample.id) throw new Error('Cannot compare different case orders.');
		return { id: sample.id, rustMedianMs: sample.medianMs, typescriptMedianMs: other.medianMs,
			rustUpperQuartileMs: sample.upperQuartileMs, typescriptUpperQuartileMs: other.upperQuartileMs,
			...(sample.p95Ms !== undefined ? { rustP95Ms: sample.p95Ms } : {}),
			...(other.p95Ms !== undefined ? { typescriptP95Ms: other.p95Ms } : {}),
			speedup: other.medianMs / sample.medianMs, rustSlowerPercent: (sample.medianMs / other.medianMs - 1) * 100 };
	});
	return { speedup: reference.corpusMedianMs / native.corpusMedianMs,
		pageMedianSpeedup: median(perCase.map(sample => sample.speedup)), perCase,
		rustSlowerCases: perCase.filter(sample => sample.speedup < 1).sort((a, b) => a.speedup - b.speedup) };
}
