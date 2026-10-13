/** Deterministic formatting through the pinned TypeScript parser/printer. */
import ts from 'typescript';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });
function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory())
            visit(path);
        else if (/\.(?:ts|mjs|js)$/.test(entry.name)) {
            const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true, path.endsWith('.ts') ? ts.ScriptKind.TS : ts.ScriptKind.JS);
            if (source.parseDiagnostics.length)
                throw new Error(`Refusing to format invalid source: ${path}`);
            writeFileSync(path, printer.printFile(source));
        }
    }
}
for (const folder of ['src', 'test', 'scripts', 'benchmarks', 'workers'])
    visit(folder);
