#!/usr/bin/env node
/** Publish this repository using the owner's locally authenticated GitHub CLI.
 * No token is read, printed, embedded in a URL or stored by this script.
 */
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const OWNER = 'HECer';
const REPO = `${OWNER}/Code-Intelligence`;
const HTTPS = `https://github.com/${REPO}.git`;
const SSH = `git@github.com:${REPO}.git`;
const dryRun = process.argv.includes('--dry-run');
const args = process.argv.slice(2);
if (args.some(arg => arg !== '--dry-run'))
    throw new Error('Only --dry-run is supported.');
function run(command, args, { allowFailure = false } = {}) {
    const result = spawnSync(command, args, { cwd: ROOT, encoding: 'utf8', shell: false,
        maxBuffer: 8000000, timeout: 180000, env: { ...process.env, GH_PROMPT_DISABLED: '1' } });
    if (result.error)
        throw new Error(`${command}: ${result.error.code ?? result.error.message}. Install Git and GitHub CLI, then run gh auth login locally.`);
    if (result.status !== 0 && !allowFailure)
        throw new Error(`${command} ${args.join(' ')} failed:\n${result.stderr || result.stdout}`);
    return { ok: result.status === 0, stdout: result.stdout.trim(), stderr: result.stderr.trim() };
}
try {
    if (dryRun) {
        console.log(JSON.stringify({ dryRun: true, repository: REPO, visibility: 'public', changesMade: false,
            steps: ['Require a clean, committed local main branch', 'Verify gh account is HECer',
                'Create the public repository if absent; refuse an existing private repository',
                'Refuse an unexpected origin or unrelated/divergent remote history',
                'Configure GitHub CLI credential helper; push main without force',
                'Push the local v0.1.0 tag if present; verify GitHub visibility and HEAD'] }, null, 2));
        process.exit(0);
    }
    const head = run('git', ['rev-parse', 'HEAD']).stdout;
    if (run('git', ['branch', '--show-current']).stdout !== 'main')
        throw new Error('Switch to the intended main branch before publishing.');
    if (run('git', ['status', '--porcelain']).stdout)
        throw new Error('Working tree is not clean. Review and commit changes before publishing.');
    const who = run('gh', ['api', 'user', '--jq', '.login']).stdout;
    if (who.toLowerCase() !== OWNER.toLowerCase())
        throw new Error(`Authenticated account is ${who}, not ${OWNER}. Switch accounts locally with gh auth switch.`);
    const origin = run('git', ['remote', 'get-url', 'origin'], { allowFailure: true });
    if (origin.ok && ![HTTPS, SSH, HTTPS.slice(0, -4)].includes(origin.stdout))
        throw new Error('Unexpected origin; refusing to replace a remote.');
    let metadata = run('gh', ['api', `repos/${REPO}`], { allowFailure: true });
    if (!metadata.ok) {
        if (!/404|Not Found/i.test(metadata.stderr + metadata.stdout))
            throw new Error('Cannot inspect target repository. Check connection and permissions.');
        run('gh', ['repo', 'create', REPO, '--public', '--description', 'Local, evidence-first code intelligence for coding agents: snapshots, semantic navigation, directed graphs and reviewable changes.']);
        metadata = run('gh', ['api', `repos/${REPO}`]);
    }
    const repo = JSON.parse(metadata.stdout);
    if (repo.private || repo.owner?.login?.toLowerCase() !== OWNER.toLowerCase())
        throw new Error('Target is private or owned by another account; no visibility or ownership change will be attempted.');
    if (!origin.ok)
        run('git', ['remote', 'add', 'origin', HTTPS]);
    run('gh', ['auth', 'setup-git']);
    const branches = run('git', ['ls-remote', '--heads', 'origin']).stdout;
    if (branches) {
        if (!branches.split('\n').some(line => line.endsWith('\trefs/heads/main')))
            throw new Error('Existing target has other history but no main; refusing to introduce an unrelated root.');
        run('git', ['fetch', '--no-tags', 'origin', 'main']);
        if (!run('git', ['merge-base', '--is-ancestor', 'FETCH_HEAD', head], { allowFailure: true }).ok)
            throw new Error('Remote main is unrelated or ahead/diverged. Review it; this script never force-pushes.');
    }
    run('git', ['push', '-u', 'origin', 'HEAD:main']);
    if (run('git', ['rev-parse', '--verify', 'refs/tags/v0.1.0'], { allowFailure: true }).ok)
        run('git', ['push', 'origin', 'refs/tags/v0.1.0']);
    const remoteHead = run('gh', ['api', `repos/${REPO}/commits/main`, '--jq', '.sha']).stdout;
    const finalRepo = JSON.parse(run('gh', ['api', `repos/${REPO}`]).stdout);
    if (remoteHead !== head || finalRepo.private)
        throw new Error('Post-publication verification failed; inspect GitHub before retrying.');
    console.log(JSON.stringify({ published: true, repository: REPO, public: true, commit: head, url: finalRepo.html_url }, null, 2));
}
catch (error) {
    console.error(`Publication stopped: ${error.message}`);
    process.exitCode = 1;
}
