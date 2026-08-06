import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { it } from 'node:test';
import { setPublishVersion } from './set-publish-version.js';

const packageFiles = [
	'package.json',
	'apps/ccusage/package.json',
	'packages/ccusage-linux-x64/package.json',
	'packages/ccusage-darwin-arm64/package.json',
	'packages/ccusage-win32-x64/package.json',
] as const;

void it('writes one run-specific version to every published package', async () => {
	const repoRoot = await mkdtemp(path.join(tmpdir(), 'ccusage-publish-version-'));
	try {
		for (const packageFile of packageFiles) {
			const absolutePath = path.join(repoRoot, packageFile);
			await mkdir(path.dirname(absolutePath), { recursive: true });
			await writeFile(
				absolutePath,
				`${JSON.stringify({ name: packageFile, version: '20.0.19' }, null, '\t')}\n`,
			);
		}

		const version = await setPublishVersion({
			repoRoot,
			runAttempt: '2',
			runId: '31108070276',
		});

		assert.equal(version, '20.0.20-fork.31108070276.2');
		for (const packageFile of packageFiles) {
			const manifest = JSON.parse(await readFile(path.join(repoRoot, packageFile), 'utf8')) as {
				version: string;
			};
			assert.equal(manifest.version, version);
		}
	} finally {
		await rm(repoRoot, { force: true, recursive: true });
	}
});
