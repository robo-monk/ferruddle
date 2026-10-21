import { test, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { normalizeHtmlAttributes, normalizeMarkdownHtml } from '../../tests/helpers';
import { PARITY_FIELDS, sha256, type OracleCase, type ParityResult } from './shared';

test('compare Rust JSONL results with the complete pinned oracle', () => {
	if (!process.env.DEFUDDLE_ORACLE_DIR || !process.env.DEFUDDLE_RUST_RESULTS) {
		throw new Error('Set DEFUDDLE_ORACLE_DIR and DEFUDDLE_RUST_RESULTS.');
	}
	const root = resolve(process.env.DEFUDDLE_ORACLE_DIR);
	const manifest = JSON.parse(readFileSync(join(root, 'manifest.json'), 'utf8'));
	expect(manifest.schemaVersion).toBe(1);
	expect(manifest.parityFields).toEqual(PARITY_FIELDS);
	expect(manifest.caseCount).toBeGreaterThan(0);
	expect(manifest.cases).toHaveLength(manifest.caseCount);
	const actual = new Map<string, ParityResult>();
	const lines = readFileSync(process.env.DEFUDDLE_RUST_RESULTS, 'utf8').split('\n').filter(line => line.trim());
	for (const line of lines) {
		const item = JSON.parse(line);
		expect(typeof item.id).toBe('string');
		expect(actual.has(item.id), `duplicate result: ${item.id}`).toBe(false);
		expect(item.result && typeof item.result === 'object', item.id).toBeTruthy();
		actual.set(item.id, item.result);
	}
	expect(new Set(manifest.cases.map((entry: { id: string }) => entry.id)).size).toBe(manifest.caseCount);
	expect([...actual.keys()].sort()).toEqual(manifest.cases.map((entry: { id: string }) => entry.id).sort());
	for (const entry of manifest.cases) {
		const raw = readFileSync(join(root, entry.file), 'utf8');
		expect(sha256(raw), entry.id).toBe(entry.sha256);
		const reference: OracleCase = JSON.parse(raw);
		expect(reference.id).toBe(entry.id);
		expect(sha256(reference.inputHtml), entry.id).toBe(reference.inputSha256);
		const result = actual.get(entry.id)!;
		for (const field of PARITY_FIELDS) {
			expect(typeof result[field], `${entry.id}: ${field} type`).toBe(typeof reference.expected[field]);
			if (field === 'content') {
				expect(normalizeHtmlAttributes(result.content.trim()), `${entry.id}: HTML`)
					.toBe(normalizeHtmlAttributes(reference.expected.content.trim()));
			} else if (field === 'contentMarkdown') {
				// Upstream trims the combined metadata preamble + Markdown, so only
				// trailing Markdown whitespace is ignored; leading body space matters.
				expect(normalizeMarkdownHtml(result.contentMarkdown.trimEnd()), `${entry.id}: Markdown`)
					.toBe(normalizeMarkdownHtml(reference.expected.contentMarkdown.trimEnd()));
			} else {
				expect(result[field], `${entry.id}: ${field}`).toEqual(reference.expected[field]);
			}
		}
	}
	console.log(`Matched ${manifest.caseCount}/${manifest.caseCount} oracle cases (${manifest.dom}).`);
});
