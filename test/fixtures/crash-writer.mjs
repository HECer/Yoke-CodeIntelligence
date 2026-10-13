import { CodeIntelligence } from '../../dist/engine.js';
import { ChangeManager } from '../../dist/changes/manager.js';
import { Deadline } from '../../dist/core/util.js';
const engine = new CodeIntelligence(process.argv[2], { config: { timeoutMs: 120000, maxResultBytes: 300000 } });
const changes = new ChangeManager(engine.workspace, { allowWrites: true, afterWrite() { process.exit(73); } });
await changes.apply(process.argv[3], 'crash', 'workspace', new Deadline(120000));
process.exit(74);
