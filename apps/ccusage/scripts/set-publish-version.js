import { appendFile, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const packageFiles = [
	'package.json',
	'apps/ccusage/package.json',
	'packages/ccusage-linux-x64/package.json',
	'packages/ccusage-darwin-arm64/package.json',
	'packages/ccusage-win32-x64/package.json',
];

export async function setPublishVersion({ repoRoot, runAttempt, runId }) {
	const packages = await Promise.all(
		packageFiles.map(async (packageFile) => {
			const absolutePath = path.join(repoRoot, packageFile);
			const manifest = JSON.parse(await readFile(absolutePath, 'utf8'));
			return { absolutePath, manifest };
		}),
	);
	const versions = new Set(packages.map(({ manifest }) => manifest.version));
	if (versions.size !== 1) {
		throw new Error(`package versions are not synchronized: ${[...versions].join(', ')}`);
	}

	const checkedInVersion = [...versions][0].split('-', 1)[0];
	const [major, minor, patch] = checkedInVersion.split('.').map(Number);
	if (![major, minor, patch].every(Number.isSafeInteger)) {
		throw new Error(`invalid checked-in version: ${checkedInVersion}`);
	}
	if (!/^\d+$/.test(runId) || !/^\d+$/.test(runAttempt)) {
		throw new Error('runId and runAttempt must be decimal integers');
	}

	const version = `${major}.${minor}.${patch + 1}-fork.${runId}.${runAttempt}`;
	await Promise.all(
		packages.map(async ({ absolutePath, manifest }) => {
			manifest.version = version;
			await writeFile(absolutePath, `${JSON.stringify(manifest, null, '\t')}\n`);
		}),
	);
	return version;
}

async function main() {
	const version = await setPublishVersion({
		repoRoot: process.cwd(),
		runAttempt: process.env.RUN_ATTEMPT ?? '',
		runId: process.env.RUN_ID ?? '',
	});
	if (process.env.GITHUB_ENV) {
		await appendFile(process.env.GITHUB_ENV, `NPM_VERSION=${version}\n`);
	}
	console.log(`Publishing ${version}`);
}

const entryPoint = process.argv[1];
if (entryPoint && import.meta.url === pathToFileURL(path.resolve(entryPoint)).href) {
	await main();
}
