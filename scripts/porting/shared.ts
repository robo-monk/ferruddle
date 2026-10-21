import { createHash } from 'node:crypto';
import type { DefuddleResponse } from '../../src/types';

export const PARITY_FIELDS = [
	'content', 'contentMarkdown', 'title', 'author', 'site', 'published', 'wordCount', 'extractorType',
] as const;

export function projectResult(result: DefuddleResponse) {
	return {
		content: result.content,
		contentMarkdown: result.contentMarkdown ?? '',
		title: result.title,
		author: result.author,
		site: result.site,
		published: result.published,
		wordCount: result.wordCount,
		extractorType: result.extractorType ?? null,
	};
}

export type ParityResult = ReturnType<typeof projectResult>;

export interface OracleCase {
	id: string;
	url: string;
	inputHtml: string;
	inputSha256: string;
	expected: ParityResult;
}

export const sha256 = (value: string | Buffer) => createHash('sha256').update(value).digest('hex');
