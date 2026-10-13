import { lstatSync, realpathSync, existsSync } from 'node:fs';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { assert } from './util.js';
export const STATE = '.code-intelligence';
const blockedDirs = new Set(['.git', '.hg', '.svn', STATE, 'node_modules', '.venv', 'venv', '__pycache__']);
export function normalizePath(value: string): string {
    assert(value.length > 0 && value.length < 4096 && !/[\x00-\x1f]/.test(value), 'PATH_DENIED', 'Invalid path');
    assert(!isAbsolute(value) && !/^[A-Za-z]:|^[/\\]/.test(value), 'PATH_DENIED', 'An explicit workspace-relative path is required');
    const out = value.replaceAll('\\', '/');
    assert(out.split('/').every(p => p && p !== '.' && p !== '..' && !p.includes(':') && !/[. ]$/.test(p)), 'PATH_DENIED', 'Unsafe path component');
    assert(!out.split('/').some(p => /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(p)), 'PATH_DENIED', 'Reserved device name');
    return out;
}
export function blockedPath(path: string): boolean {
    const segments = path.split('/');
    return segments.some(s => blockedDirs.has(s.toLowerCase())) || segments.some(s => /^\.env(?:\.|$)|^\.npmrc$|^\.pypirc$|^credentials(?:\.|$)|^id_(rsa|ed25519)$/i.test(s)) || /\.(pem|key|p12|pfx|keystore)$/i.test(path);
}
export function inside(root: string, path: string): boolean { const rel = relative(root, path); return rel === '' || (!isAbsolute(rel) && rel !== '..' && !rel.startsWith(`..${sep}`)); }
export function safePath(root: string, path: string, allowState = false): string {
    const normalized = normalizePath(path);
    assert(allowState || !blockedPath(normalized), 'PATH_DENIED', 'Protected path is not accessible');
    const base = realpathSync(root);
    let current = base;
    for (const part of normalized.split('/')) {
        current = resolve(current, part);
        try {
            const stat = lstatSync(current);
            assert(!stat.isSymbolicLink(), 'PATH_DENIED', 'Symlinks and junctions are not followed');
            assert(inside(base, realpathSync(current)), 'PATH_DENIED', 'Path escapes workspace');
        }
        catch (e) {
            if ((e as NodeJS.ErrnoException).code !== 'ENOENT')
                throw e;
        }
    }
    assert(inside(base, current), 'PATH_DENIED', 'Path escapes workspace');
    return current;
}
export function glob(pattern: string): RegExp {
    let out = '';
    for (let i = 0; i < pattern.length; i++) {
        const c = pattern[i]!;
        if (c === '*') {
            if (pattern[i + 1] === '*') {
                i++;
                if (pattern[i + 1] === '/') {
                    i++;
                    out += '(?:.*/)?';
                }
                else
                    out += '.*';
            }
            else
                out += '[^/]*';
        }
        else if (c === '?')
            out += '[^/]';
        else
            out += c.replace(/[.+^${}()|[\]\\]/g, '\\$&');
    }
    return new RegExp('^' + out + '$');
}
export function language(path: string): string {
    if (/\.[cm]?tsx?$/.test(path))
        return 'typescript';
    if (/\.[cm]?jsx?$/.test(path))
        return 'javascript';
    if (/\.pyi?$/.test(path))
        return 'python';
    if (/\.(c|h)$/.test(path))
        return 'c';
    if (/\.(cpp|cxx|cc|hpp|hxx)$/.test(path))
        return 'cpp';
    if (/\.rs$/.test(path))
        return 'rust';
    if (/\.java$/.test(path))
        return 'java';
    if (/\.cs$/.test(path))
        return 'csharp';
    if (/\.go$/.test(path))
        return 'go';
    if (/\.(md|mdx|rst|txt)$/i.test(path))
        return 'document';
    if (/\.sql$/i.test(path))
        return 'sql';
    if (/\.(json|jsonc|yaml|yml|toml|proto|graphql|gql)$/i.test(path))
        return 'config';
    return 'text';
}
export function secretLike(text: string): boolean { return /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9_]{30,}\b|\bgithub_pat_[A-Za-z0-9_]{30,}\b|\bAKIA[0-9A-Z]{16}\b/.test(text); }
