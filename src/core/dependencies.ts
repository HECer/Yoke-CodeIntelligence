/** Capture installed declarations as immutable, read-only inputs. No package code is run. */
import { existsSync, lstatSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { safePath, blockedPath, normalizePath } from './paths.js';
import { assert } from './util.js';
export function dependencyPath(root: string, value: string): string {
    const path = normalizePath(value);
    assert(path.split('/').includes('node_modules'), 'PATH_DENIED', 'Dependency path must be inside node_modules');
    assert(!blockedPath(path.split('/').filter(p => p !== 'node_modules').join('/')), 'PATH_DENIED', 'Protected dependency path');
    assert(/(?:^|\/)package\.json$|\.d\.[cm]?ts$|(?:^|\/)tsconfig[^/]*\.json$/i.test(path), 'PATH_DENIED', 'Only dependency declarations and package/config metadata may be captured');
    return safePath(root, path, true);
}
export function dependencyFiles(root: string, maximum: number): {
    paths: string[];
    omissions: {
        path: string;
        reason: string;
    }[];
} {
    const paths: string[] = [], omissions: {
        path: string;
        reason: string;
    }[] = [];
    // Discover nested workspace node_modules too, while excluding build/state trees.
    let visited = 0, capped = false;
    const visit = (relative: string, inDependency: boolean, depth: number): void => {
        if (capped || depth > 64) {
            if (depth > 64)
                omissions.push({ path: relative, reason: 'dependency_depth_limit' });
            return;
        }
        const directory = join(root, relative);
        if (!existsSync(directory))
            return;
        if (lstatSync(directory).isSymbolicLink()) {
            omissions.push({ path: relative, reason: 'dependency_symlink_not_followed' });
            return;
        }
        if (++visited > 50000) {
            capped = true;
            omissions.push({ path: relative, reason: 'dependency_directory_limit' });
            return;
        }
        for (const item of readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
            if (capped)
                break;
            const path = relative ? `${relative}/${item.name}` : item.name;
            if (item.name.startsWith('.') || ['dist', 'build', 'coverage', 'vendor', '__pycache__', 'venv'].includes(item.name) && !inDependency)
                continue;
            const nextDependency = inDependency || item.name === 'node_modules';
            if (item.isSymbolicLink()) {
                if (nextDependency)
                    omissions.push({ path, reason: 'dependency_symlink_not_followed' });
                continue;
            }
            if (item.isDirectory())
                visit(path, nextDependency, depth + 1);
            else if (inDependency && item.isFile() && /(?:^|\/)package\.json$|\.d\.[cm]?ts$|(?:^|\/)tsconfig[^/]*\.json$/i.test(path)) {
                if (paths.length >= maximum) {
                    capped = true;
                    omissions.push({ path, reason: 'dependency_file_limit' });
                    break;
                }
                try {
                    dependencyPath(root, path);
                    paths.push(path);
                }
                catch {
                    omissions.push({ path, reason: 'dependency_path_denied' });
                }
            }
        }
    };
    visit('', false, 0);
    return { paths: paths.sort(), omissions };
}
