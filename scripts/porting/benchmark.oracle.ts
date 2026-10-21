import { test, vi } from 'vitest';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { Defuddle } from '../../src/node';
import type { DefuddleResponse } from '../../src/types';
import { parseDocument } from '../../tests/helpers';
import { projectResult, sha256, type OracleCase } from './shared';

type SelectedCase = OracleCase & { synchronousDifferenceFields?: string[] };
type OutputProof = { sha256: string; contentBytes: number; markdownBytes: number; wordCount: number };

test('measure selected offline inputs with validation outside all timers', async () => {
	if (process.env.TZ !== 'UTC') throw new Error('Run with TZ=UTC.');
	if (process.env.DOM === 'jsdom') throw new Error('This benchmark uses the production linkedom backend.');
	const selection = JSON.parse(readFileSync(process.env.DEFUDDLE_BENCH_SELECTION!, 'utf8'));
	const directory = resolve(__dirname, '../../rust/tests/data/upstream');
	const manifestBytes = readFileSync(resolve(directory, 'manifest.json'));
	if (sha256(manifestBytes) !== selection.manifestSha256) throw new Error('Changed manifest.');
	const manifest = JSON.parse(manifestBytes.toString());
	const mathBytes = readFileSync(resolve(directory, '../quality/math.json'));
	if (sha256(mathBytes) !== selection.mathExclusionsSha256) throw new Error('Changed math exclusions.');
	const mathIds = Object.keys(JSON.parse(mathBytes.toString())).sort();
	if (JSON.stringify(mathIds) !== JSON.stringify(selection.excludedMathCaseIds)) throw new Error('Changed math IDs.');
	const warm = selection.kind === 'warm';
	if (selection.schemaVersion !== 2 || (!warm && selection.kind !== 'cold')
		|| (warm && (selection.caseIds.length !== 208 || selection.rounds < 3 || selection.warmups < 1))
		|| (!warm && (selection.caseIds.length !== 1 || selection.rounds !== 1 || selection.warmups !== 0))
		|| selection.inputs.length !== selection.caseIds.length
		|| selection.modeOrder.length !== (warm ? 2 : 1)
		|| new Set(selection.modeOrder).size !== selection.modeOrder.length
		|| selection.modeOrder.some((mode: string) => !['html', 'html+markdown'].includes(mode))) {
		throw new Error('Invalid benchmark selection dimensions.');
	}
	const expectedIds = manifest.cases.map((entry: { id: string }) => entry.id).filter((id: string) => !mathIds.includes(id));
	if ((warm && JSON.stringify(expectedIds) !== JSON.stringify(selection.caseIds))
		|| selection.caseIds.some((id: string) => mathIds.includes(id))) throw new Error('Keep the complete ordered non-math corpus.');
	const cases: SelectedCase[] = selection.inputs.map((spec: {
		id: string; file: string; sha256: string; inputSha256: string; url: string;
	}, index: number) => {
		const entry = manifest.cases.find((entry: { id: string }) => entry.id === spec.id);
		if (!entry || selection.caseIds[index] !== spec.id || entry.file !== spec.file || entry.sha256 !== spec.sha256) {
			throw new Error(`Unknown or changed selected case ${spec.id}`);
		}
		const bytes = readFileSync(resolve(directory, entry.file));
		if (sha256(bytes) !== spec.sha256) throw new Error(`Changed case ${spec.id}`);
		const item = JSON.parse(bytes.toString());
		if (item.id !== spec.id || item.url !== spec.url || !item.url
			|| item.inputSha256 !== spec.inputSha256 || sha256(item.inputHtml) !== spec.inputSha256) {
			throw new Error(`Changed input HTML/URL ${spec.id}`);
		}
		return item;
	});
	const identity = cases.map(({ id, url, inputSha256 }) => ({ id, url, inputSha256 }));
	if (sha256(JSON.stringify(identity)) !== selection.inputSetSha256) throw new Error('Changed input identity.');
	if (process.env.DEFUDDLE_BENCH_VALIDATE_ONLY === '1') return;

	const rounds: number = selection.rounds;
	const warmups: number = selection.warmups;
	let fetchAttempts = 0;
	const offlineFetch: typeof fetch = async () => { fetchAttempts++; throw new Error('benchmark network disabled'); };
	vi.stubGlobal('fetch', offlineFetch);
	const modes = [];
	try {
		for (const mode of selection.modeOrder as string[]) {
			const markdown = mode === 'html+markdown';
			const options = { useAsync: false, separateMarkdown: markdown, fetch: offlineFetch };
			const samples: number[] = [];
			const perCase = cases.map(() => [] as number[]);
			const proofs: (OutputProof | undefined)[] = new Array(cases.length);
			const rawRounds = [];
			for (let round = 0; round < warmups + rounds; round++) {
				const outputs: DefuddleResponse[] = new Array(cases.length);
				const caseMs: number[] = new Array(cases.length);
				const start = performance.now();
				for (let offset = 0; offset < cases.length; offset++) {
					const index = (offset + round) % cases.length;
					const item = cases[index];
					const before = performance.now();
					const result = await Defuddle(parseDocument(item.inputHtml, item.url), item.url, options);
					caseMs[index] = performance.now() - before;
					outputs[index] = result;
				}
				const wallMs = performance.now() - start;
				// Output traversal, reference comparisons, hashing and storage happen
				// only after the full-corpus timer and every per-case timer stop.
				for (const [index, result] of outputs.entries()) {
					const item = cases[index];
					const projected = projectResult(result);
					if (!projected.content || (!markdown && projected.contentMarkdown)) throw new Error(`Invalid output mode: ${item.id}`);
					const expected = { ...item.expected, contentMarkdown: markdown ? item.expected.contentMarkdown : '' };
					for (const field of Object.keys(expected) as (keyof typeof expected)[]) {
						if (!item.synchronousDifferenceFields?.includes(field) && projected[field] !== expected[field]) {
							throw new Error(`TypeScript benchmark output differs from pinned oracle: ${item.id}.${field}`);
						}
					}
					const proof = { sha256: sha256(JSON.stringify(projected)), contentBytes: Buffer.byteLength(projected.content),
						markdownBytes: Buffer.byteLength(projected.contentMarkdown), wordCount: projected.wordCount };
					if (proofs[index] && JSON.stringify(proofs[index]) !== JSON.stringify(proof)) {
						throw new Error(`Nondeterministic output: ${item.id} (${mode})`);
					}
					proofs[index] = proof;
				}
				const measured = round >= warmups;
				if (measured) {
					samples.push(wallMs);
					caseMs.forEach((ms, index) => perCase[index].push(ms));
				}
				rawRounds.push({ round, phase: measured ? 'measured' : 'warmup', orderStart: round % cases.length, wallMs, caseMs });
			}
			modes.push({ mode, roundTotalMs: samples, rawRounds,
				perCase: cases.map((item, index) => ({ id: item.id, ms: perCase[index], output: proofs[index] })) });
		}
	} finally { vi.unstubAllGlobals(); }
	if (fetchAttempts) throw new Error(`Unexpected ${fetchAttempts} network attempts in synchronous benchmark`);
	writeFileSync(process.env.DEFUDDLE_BENCH_OUTPUT!, JSON.stringify({ schemaVersion: 2, implementation: 'typescript',
		kind: selection.kind, pass: selection.pass, caseCount: cases.length, rounds, warmups,
		manifestSha256: selection.manifestSha256, inputSetSha256: selection.inputSetSha256,
		modeOrder: selection.modeOrder, nodeVersion: process.version, dom: 'linkedom', modes }, null, 2) + '\n');
});
