import { test, expect, vi } from 'vitest';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { Defuddle } from '../../src/node';
import { getFixtures, normalizeHtmlAttributes, normalizeMarkdownHtml, parseDocument } from '../../tests/helpers';
import { PARITY_FIELDS, projectResult, sha256 } from './shared';

test('export a portable oracle only after validating every upstream fixture', async () => {
	const output = process.env.DEFUDDLE_ORACLE_DIR;
	if (!output) throw new Error('Set DEFUDDLE_ORACLE_DIR to a new output directory.');
	if (process.env.TZ !== 'UTC') throw new Error('Run with TZ=UTC, matching the upstream tests.');
	const outputDir = resolve(output);
	if (existsSync(outputDir)) throw new Error(`Output directory already exists: ${outputDir}`);
	const root = resolve(__dirname, '../..');
	const dom = process.env.DOM === 'jsdom' ? 'jsdom' : 'linkedom';
	const cases = [];
	const fixtures = getFixtures().sort((a, b) => a.name.localeCompare(b.name, 'en'));
	expect(fixtures.length).toBeGreaterThan(0);
	let attempts = 0;
	const offlineFetch: typeof fetch = async () => {
		attempts++;
		throw new Error('network disabled in fixture tests');
	};
	// Guard both the documented injection point and accidental global fetches.
	vi.stubGlobal('fetch', offlineFetch);
	try {
		for (const { name, path } of fixtures) {
			const inputHtml = readFileSync(path, 'utf8');
			// Keep URL derivation identical to tests/fixtures.test.ts, including its
			// single-line JSON comment convention and filename fallback.
			const match = inputHtml.match(/<!--\s*(\{"url":.*?\})\s*-->/);
			const frontmatter = match ? JSON.parse(match[1]) : {};
			const url = frontmatter.url || `https://${basename(path, '.html').replace(/^[a-z]+--/, '')}`;
			const response = await Defuddle(parseDocument(inputHtml, url), url, {
				separateMarkdown: true, fetch: offlineFetch,
			});
			const expectedPath = join(root, 'tests/expected', `${name}.md`);
			// Deliberately fail on missing expectations; never bless new baselines.
			const golden = readFileSync(expectedPath, 'utf8');
			const metadata = { title: response.title, author: response.author, site: response.site, published: response.published };
			const comparable = '```json\n' + JSON.stringify(metadata, null, 2) + '\n```\n\n' + response.contentMarkdown;
			expect(response.content.length, name).toBeGreaterThan(0);
			expect(response.contentMarkdown?.length, name).toBeGreaterThan(0);
			expect(normalizeMarkdownHtml(comparable.trim()), name).toBe(normalizeMarkdownHtml(golden.trim()));
			const htmlPath = join(root, 'tests/expected', `${name}.html`);
			const htmlGolden = existsSync(htmlPath) ? readFileSync(htmlPath, 'utf8') : null;
			if (htmlGolden !== null) {
				expect(normalizeHtmlAttributes(response.content.trim()), name).toBe(normalizeHtmlAttributes(htmlGolden.trim()));
			}
			// Establish whether a synchronous, network-free API can reproduce the
			// actual fixture contract. This is evidence, not an assumed equivalence.
			const syncResponse = await Defuddle(parseDocument(inputHtml, url), url, {
				separateMarkdown: true, useAsync: false, fetch: offlineFetch,
			});
			const expected = projectResult(response);
			const sync = projectResult(syncResponse);
			cases.push({
				id: name, url, inputHtml, inputSha256: sha256(inputHtml),
				upstreamFixture: relative(root, path),
				markdownGoldenSha256: sha256(golden),
				htmlGoldenSha256: htmlGolden === null ? null : sha256(htmlGolden),
				expected,
				synchronousDifferenceFields: PARITY_FIELDS.filter(field => expected[field] !== sync[field]),
			});
		}
	} finally {
		vi.unstubAllGlobals();
	}

	function sourceFiles(directory: string): string[] {
		return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
			const path = join(directory, entry.name);
			return entry.isDirectory() ? sourceFiles(path) : path.endsWith('.ts') ? [path] : [];
		});
	}
	const provenance = [
		...sourceFiles(join(root, 'src')), ...sourceFiles(join(root, 'scripts/porting')),
		...['package.json', 'package-lock.json', 'vitest.config.ts', 'tests/helpers.ts', 'tests/fixtures.test.ts']
			.map(path => join(root, path)),
	].sort();
	const sourceHashes = Object.fromEntries(provenance.map(path => [relative(root, path), sha256(readFileSync(path))]));
	const manifest = {
		schemaVersion: 1,
		upstreamRevision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
		upstreamVersion: JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version,
		dom, nodeVersion: process.version, timezone: process.env.TZ,
		domVersion: JSON.parse(readFileSync(join(root, 'node_modules', dom, 'package.json'), 'utf8')).version,
		options: { separateMarkdown: true }, network: 'reject-all-fetches', fetchAttempts: attempts,
		caseCount: cases.length,
		checkedHtmlGoldenCount: cases.filter(item => item.htmlGoldenSha256 !== null).length,
		synchronousDifferences: cases.filter(item => item.synchronousDifferenceFields.length > 0)
			.map(item => ({ id: item.id, fields: item.synchronousDifferenceFields })),
		parityFields: PARITY_FIELDS, sourceHashes,
		cases: cases.map(item => {
			const file = `${item.id}.json`;
			return { id: item.id, file, sha256: sha256(JSON.stringify(item, null, 2) + '\n') };
		}),
	};
	// No output is written until every existing expectation has passed.
	mkdirSync(dirname(outputDir), { recursive: true });
	mkdirSync(outputDir);
	for (const item of cases) writeFileSync(join(outputDir, `${item.id}.json`), JSON.stringify(item, null, 2) + '\n');
	writeFileSync(join(outputDir, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
	console.log(JSON.stringify({ outputDir, caseCount: manifest.caseCount, dom, checkedHtmlGoldenCount: manifest.checkedHtmlGoldenCount, synchronousDifferences: manifest.synchronousDifferences }));
});
