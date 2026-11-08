// Independently owned experimental own_v1 fixture. Not a product provider.
// Source declarations are read as UTF-8 bytes; LSP coordinates use UTF-16.
import { appendFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { TextDecoder } from 'node:util';

const [transcriptPath, scenario = 'valid', ...extraArgs] = process.argv.slice(2);
const scenarios = new Set([
  'valid', 'copy_only', 'delete_only', 'body_change', 'noop',
  'wrong_overload', 'wrong_parent', 'whole_file', 'virtual', 'resource',
  'whole_source', 'whole_target', 'split_cut', 'repeated_original',
  'resource_noop', 'duplicate_copy', 'replace_parent', 'nested_inner',
  'shift_before_parent', 'invalid_third_rust',
  'delete_root_preamble', 'delete_module_preamble',
  'delete_outer_attachment', 'transfer_outer_attachment',
  'transfer_parent_attachment',
  'delete_source_preamble', 'transfer_source_attachment',
  'update_attributed_use', 'update_attributed_const',
  'wrapped_source_copy', 'wrapped_target_copy', 'literal_decl_text',
  'unsupported', 'ack_only', 'command_only', 'wrong_request_id',
  'wrong_schema', 'extra_response_field', 'missing_capability',
  'bad_capability_version',
  'missing_attachment_capability', 'bad_attachment_capability',
  'node_only_transfer', 'strip_transferred_attachments',
  'crlf_padding',
]);
if (!transcriptPath || extraArgs.length || !scenarios.has(scenario)) {
  process.stderr.write('Usage: node own-refactor-intent-lsp.mjs <transcriptPath> [scenario]\n');
  process.exit(2);
}

const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const documents = new Map();
let rootPath = null;
let initialized = false;
let shutdown = false;
let input = Buffer.alloc(0);
let framingFailed = false;
const MAX_HEADER_BYTES = 64 * 1024;
const MAX_MESSAGE_BYTES = 16 * 1024 * 1024;

function invalid(message) {
  const error = new Error(message);
  error.rpcCode = -32602;
  throw error;
}

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function requiredString(value, label) {
  if (typeof value !== 'string' || value.length === 0) invalid(`${label} must be nonempty`);
  return value;
}

function canonicalRelative(value) {
  requiredString(value, 'binding.path');
  if (value.includes('\\') || value.includes(':') || value.includes('\0') ||
      value.startsWith('/') || value.split('/').some(part => part === '' || part === '.' || part === '..')) {
    invalid('binding.path must be a canonical relative source path');
  }
  return value;
}

function byteOffset(bytes, value, label) {
  if (!Number.isSafeInteger(value) || value < 0 || value > bytes.length) invalid(`${label} is out of bounds`);
  try {
    utf8.decode(bytes.subarray(0, value));
  } catch {
    invalid(`${label} splits a UTF-8 code point`);
  }
  if (value > 0 && value < bytes.length && bytes[value - 1] === 13 && bytes[value] === 10) {
    invalid(`${label} splits CRLF`);
  }
  return value;
}

function position(bytes, offset) {
  byteOffset(bytes, offset, 'edit offset');
  const prefix = utf8.decode(bytes.subarray(0, offset));
  const lines = prefix.split(/\r\n|\r|\n/);
  // JavaScript string length counts UTF-16 code units, including surrogate pairs.
  return { line: lines.length - 1, character: lines[lines.length - 1].length };
}

function loadDocument(uri, binding) {
  requiredString(uri, 'document URI');
  if (!object(binding)) invalid('document binding must be an object');
  const relativePath = canonicalRelative(binding.path);
  let localPath;
  try {
    const url = new URL(uri);
    if (url.protocol !== 'file:' || url.search || url.hash) invalid('document URI must be a plain file URI');
    if (url.hostname && url.hostname.toLowerCase() !== 'localhost') invalid('document URI must refer to the local machine');
    localPath = path.resolve(fileURLToPath(url));
  } catch (error) {
    if (error.rpcCode) throw error;
    invalid('document URI is not a local file URI');
  }
  if (rootPath !== null) {
    const expected = path.resolve(rootPath, ...relativePath.split('/'));
    const comparable = value => process.platform === 'win32' ? value.toLowerCase() : value;
    if (comparable(localPath) !== comparable(expected)) invalid('document URI does not match its relative path under initialize root');
  } else if (!localPath.replaceAll('\\', '/').endsWith(`/${relativePath}`)) {
    invalid('document URI does not end in its bound relative path');
  }
  const open = documents.get(uri);
  const bytes = open ? Buffer.from(open.text, 'utf8') : readFileSync(localPath);
  try {
    utf8.decode(bytes);
  } catch {
    invalid('document bytes must be valid UTF-8');
  }
  return { uri, path: relativePath, bytes, version: open ? open.version : null };
}

function sourceInterval(document, binding) {
  const start = byteOffset(document.bytes, binding.startByte, 'source.startByte');
  const end = byteOffset(document.bytes, binding.endByte, 'source.endByte');
  if (end <= start) invalid('source interval must be nonempty');
  const nameStart = byteOffset(document.bytes, binding.nameStartByte, 'source.nameStartByte');
  const nameEnd = byteOffset(document.bytes, binding.nameEndByte, 'source.nameEndByte');
  if (nameStart < start || nameEnd <= nameStart || nameEnd > end) invalid('source name must be inside its declaration');
  return { start, end, nameStart, nameEnd };
}

function edit(document, start, end, bytes) {
  return { start, end, bytes, document };
}

function textEdit(value) {
  return {
    range: { start: position(value.document.bytes, value.start), end: position(value.document.bytes, value.end) },
    newText: utf8.decode(value.bytes),
  };
}

function groupEdits(edits) {
  const groups = new Map();
  for (const value of edits) {
    let group = groups.get(value.document.path);
    if (!group) {
      group = { document: value.document, edits: [] };
      groups.set(value.document.path, group);
    } else if (group.document.uri !== value.document.uri) {
      invalid('one relative path must have exactly one URI');
    }
    group.edits.push(value);
  }
  return [...groups.values()];
}

function workspaceEdit(edits) {
  return {
    documentChanges: groupEdits(edits).map(group => ({
      textDocument: { uri: group.document.uri, version: group.document.version },
      edits: group.edits.map(textEdit),
    })),
  };
}

function applyOriginalByteEdits(document, edits) {
  let result = document.bytes;
  let nextStart = result.length;
  for (const value of [...edits].sort((left, right) => right.start - left.start || right.end - left.end)) {
    if (value.end > nextStart) invalid('fixture byte edits overlap');
    result = Buffer.concat([result.subarray(0, value.start), value.bytes, result.subarray(value.end)]);
    nextStart = value.start;
  }
  return result;
}

function siblingDeclaration(document, binding, selected) {
  // Deliberately fixture-specific, not a language analyzer or binding proof.
  // The overload corpus uses simple one-line Rust function bodies, no nested braces.
  const name = utf8.decode(document.bytes.subarray(selected.nameStart, selected.nameEnd));
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const text = utf8.decode(document.bytes);
  const matcher = new RegExp(`^[ \\t]*((?:pub(?:\\([^\\r\\n)]*\\))?[ \\t]+)?fn[ \\t]+${escaped}[ \\t]*\\([^\\r\\n{}]*\\)[^\\r\\n{}]*\\{[^{}\\r\\n]*\\})`, 'gm');
  for (const match of text.matchAll(matcher)) {
    const prefixLength = match[0].length - match[1].length;
    const start = Buffer.byteLength(text.slice(0, match.index + prefixLength), 'utf8');
    const end = start + Buffer.byteLength(match[1], 'utf8');
    if (end <= selected.start || start >= selected.end) return { start, end };
  }
  invalid(`wrong_overload needs another simple fn ${name} declaration in ${binding.path}`);
}

function refactorIntent(params) {
  if (!object(params) || params.schemaVersion !== 1) invalid('expected own schemaVersion 1');
  requiredString(params.requestId, 'requestId');
  if (!object(params.base) || !['yoke-tree-sitter-8', 'yoke-tree-sitter-9'].includes(params.base.analyzerProducer)) {
    invalid('expected an explicit supported base analyzerProducer');
  }
  for (const key of ['workspaceId', 'snapshotId', 'manifestHash', 'serverProfileHash']) requiredString(params.base[key], `base.${key}`);
  if (!object(params.intent) || params.intent.operation !== 'moveSymbol' || !object(params.documents)) invalid('expected moveSymbol intent and documents');
  const requestId = params.requestId;
  if (scenario === 'unsupported') return { schemaVersion: 1, requestId, unsupported: { code: 'unsupported_intent', message: 'fixture unsupported' } };
  if (scenario === 'ack_only') return { schemaVersion: 1, requestId, acknowledged: true };
  if (scenario === 'command_only') return { schemaVersion: 1, requestId, command: { title: 'Never execute fixture command', command: 'fixture.neverExecute', arguments: [] } };

  const sourceBinding = params.intent.source;
  const destination = params.intent.destination;
  const source = loadDocument(params.documents.sourceUri, sourceBinding);
  const nodeInterval = sourceInterval(source, sourceBinding);
  let selected = nodeInterval;
  if (params.intent.sourceTextRange && scenario !== 'node_only_transfer') {
    const start = byteOffset(source.bytes, params.intent.sourceTextRange.startByte, 'sourceTextRange.startByte');
    const end = byteOffset(source.bytes, params.intent.sourceTextRange.endByte, 'sourceTextRange.endByte');
    if (start > nodeInterval.start || end !== nodeInterval.end) invalid('transfer must include the original declaration');
    selected = { ...nodeInterval, start, end };
  }
  const declaration = source.bytes.subarray(selected.start, selected.end);
  let destinationBinding;
  if (destination?.kind === 'fileTopLevel') destinationBinding = destination.file;
  else if (destination?.kind === 'parentSymbol') destinationBinding = destination.parent;
  else invalid('unsupported destination kind');
  const target = loadDocument(params.documents.destinationUri, destinationBinding);
  if ((source.path === target.path) !== (source.uri === target.uri)) invalid('source and destination path/URI equality must agree');
  if (source.path === target.path && !source.bytes.equals(target.bytes)) invalid('same-path documents must have the same original bytes');
  // Host admission must establish a valid actual top-level Rust scope before launch.
  // EOF appending preserves destination BOM/shebang/inner attrs/docs and root items.
  // This fixture does not parse Rust or detect dangling outer docs/attributes.
  let insertionOffset = target.bytes.length;
  if (destination.kind === 'parentSymbol') {
    const parent = sourceInterval(target, destinationBinding);
    const bodyStart = byteOffset(target.bytes, destination.bodyStartByte, 'destination.bodyStartByte');
    const bodyEnd = byteOffset(target.bytes, destination.bodyEndByte, 'destination.bodyEndByte');
    if (bodyStart < parent.start || bodyEnd < bodyStart || bodyEnd > parent.end) invalid('parent body content bounds must be inside the parent declaration');
    insertionOffset = bodyEnd;
  }
  if (source.path === target.path && insertionOffset > selected.start && insertionOffset < selected.end) invalid('destination cannot be inside selected declaration');

  let deletion = edit(source, selected.start, selected.end, Buffer.alloc(0));
  let insertedDeclaration = declaration;
  if (scenario === 'strip_transferred_attachments') {
    insertedDeclaration = source.bytes.subarray(nodeInterval.start, nodeInterval.end);
  }
  if (scenario === 'wrong_overload') {
    const other = siblingDeclaration(source, sourceBinding, selected);
    deletion = edit(source, other.start, other.end, Buffer.alloc(0));
  }
  if (scenario === 'body_change') {
    const brace = declaration.indexOf(123);
    if (brace < 0) invalid('body_change requires a fixture declaration body');
    insertedDeclaration = Buffer.concat([
      declaration.subarray(0, brace + 1),
      Buffer.from(' panic!("fixture body change"); ', 'utf8'),
      declaration.subarray(brace + 1),
    ]);
  }
  if (scenario === 'wrong_parent') {
    if (destination.kind !== 'parentSymbol') invalid('wrong_parent requires a parentSymbol destination');
    insertionOffset = target.bytes.length;
  }
  if (scenario === 'nested_inner') {
    if (destination.kind !== 'parentSymbol') invalid('nested_inner requires a parentSymbol destination');
    const nested = target.bytes.indexOf(Buffer.from('pub mod nested {', 'utf8'));
    const closingBrace = nested < 0 ? -1 : target.bytes.indexOf(125, nested);
    if (closingBrace < 0) invalid('nested_inner requires the simple nested module corpus');
    insertionOffset = closingBrace;
  }
  const padding = Buffer.from(scenario === 'crlf_padding' ? '\r\n' : '\n', 'utf8');
  const insertion = edit(target, insertionOffset, insertionOffset, Buffer.concat([
    padding, insertedDeclaration, padding,
  ]));
  let edits = [deletion, insertion];
  if (scenario === 'copy_only') edits = [insertion];
  if (scenario === 'delete_only') edits = [deletion];
  if (scenario === 'noop') edits = [edit(target, target.bytes.length, target.bytes.length, Buffer.from('\n// unrelated fixture comment\n', 'utf8'))];
  if (scenario === 'split_cut') {
    edits = [
      edit(source, selected.start, selected.nameStart, Buffer.alloc(0)),
      edit(source, selected.nameStart, selected.end, Buffer.alloc(0)),
      insertion,
    ];
  }
  if (['whole_file', 'whole_source', 'whole_target'].includes(scenario)) {
    edits = groupEdits(edits).flatMap(group => {
      const broad = scenario === 'whole_file' ||
        (scenario === 'whole_source' && group.document.path === source.path) ||
        (scenario === 'whole_target' && group.document.path === target.path);
      return broad
        ? [edit(group.document, 0, group.document.bytes.length, applyOriginalByteEdits(group.document, group.edits))]
        : group.edits;
    });
  }
  if (scenario === 'duplicate_copy') {
    if (destination.kind !== 'parentSymbol' || insertionOffset === target.bytes.length) invalid('duplicate_copy requires an inline-module destination');
    edits.push(edit(target, target.bytes.length, target.bytes.length, insertion.bytes));
  }
  if (scenario === 'replace_parent') {
    if (destination.kind !== 'parentSymbol') invalid('replace_parent requires a parentSymbol destination');
    const parent = sourceInterval(target, destinationBinding);
    if (utf8.decode(target.bytes.subarray(parent.start, parent.start + 4)) !== 'pub ') invalid('replace_parent requires the public inline-module corpus');
    edits.push(edit(target, parent.start, parent.start + 4, Buffer.alloc(0)));
  }
  if (scenario === 'shift_before_parent') {
    if (destination.kind !== 'parentSymbol') invalid('shift_before_parent requires a parentSymbol destination');
    edits.push(edit(target, 0, 0, Buffer.from('// provider preceding edit\n', 'utf8')));
  }
  if (scenario === 'wrapped_source_copy') {
    edits.push(edit(source, selected.end, selected.end, Buffer.concat([
      Buffer.from('\n// recreated fixture copy\n', 'utf8'), declaration, Buffer.from('\n', 'utf8'),
    ])));
  }
  if (scenario === 'wrapped_target_copy') {
    edits.push(edit(target, 0, 0, Buffer.concat([
      Buffer.from('\n// extra fixture copy\n', 'utf8'), declaration, Buffer.from('\n', 'utf8'),
    ])));
  }
  if (scenario === 'literal_decl_text') {
    if (declaration.includes(Buffer.from('*/', 'utf8')) || declaration.includes(Buffer.from('"###', 'utf8'))) invalid('literal_decl_text requires the simple unquoted declaration corpus');
    edits.push(edit(source, selected.end, selected.end, Buffer.concat([
      Buffer.from('\n/* ', 'utf8'), declaration,
      Buffer.from(' */\npub const MOVE_TEXT: &str = r###"', 'utf8'), declaration,
      Buffer.from('"###;\n', 'utf8'),
    ])));
  }
  if (scenario === 'update_attributed_use' || scenario === 'update_attributed_const') {
    const oldPath = Buffer.from('crate::source::helper', 'utf8');
    const newPath = Buffer.from('crate::destination::bucket::helper', 'utf8');
    const ownerTexts = scenario === 'update_attributed_use'
      ? ['use crate::source::helper;']
      : ['pub const REF: fn() -> u8 = crate::source::helper;', 'pub static REF: fn() -> u8 = crate::source::helper;'];
    const owners = ownerTexts.map(value => {
      const bytes = Buffer.from(value, 'utf8');
      return { bytes, start: target.bytes.indexOf(bytes) };
    }).filter(value => value.start >= 0);
    if (owners.length !== 1) invalid('attributed reference edit requires one exact use/const/static owner');
    const owner = owners[0];
    if (target.bytes.lastIndexOf(owner.bytes) !== owner.start) invalid('attributed reference owner must be unique');
    const attachment = Buffer.from(scenario === 'update_attributed_use'
      ? '/// Reference docs\n#[allow(unused_imports)]\n'
      : '/// Reference docs\n#[allow(dead_code)]\n', 'utf8');
    const attachmentStart = owner.start - attachment.length;
    if (attachmentStart < 0 || !target.bytes.subarray(attachmentStart, owner.start).equals(attachment)) invalid('attributed reference owner requires its unchanged exact attachment');
    const pathStart = owner.start + owner.bytes.indexOf(oldPath);
    if (target.bytes.indexOf(oldPath) !== pathStart || target.bytes.lastIndexOf(oldPath) !== pathStart) invalid('attributed reference path must be unique');
    edits.push(edit(target, pathStart, pathStart + oldPath.length, newPath));
  }
  if (scenario === 'invalid_third_rust') {
    const reference = loadDocument(new URL('reference.rs', target.uri).href, { path: 'src/reference.rs' });
    edits.push(edit(reference, 0, reference.bytes.length, Buffer.from('pub fn broken( {\n', 'utf8')));
  }
  if (scenario === 'delete_root_preamble' || scenario === 'delete_module_preamble') {
    if (destination.kind !== 'parentSymbol') invalid('preamble deletion requires a parentSymbol destination');
    const marker = Buffer.from('#![allow(dead_code)]\n', 'utf8');
    const start = target.bytes.indexOf(marker);
    if (start < 0 || start !== target.bytes.lastIndexOf(marker)) invalid('preamble deletion requires one exact inner-attribute marker');
    const parent = sourceInterval(target, destinationBinding);
    if (scenario === 'delete_root_preamble' && start >= parent.start) invalid('root preamble must precede the destination module');
    if (scenario === 'delete_module_preamble' && (start < destination.bodyStartByte || start + marker.length > destination.bodyEndByte)) invalid('module preamble must be inside the destination body');
    edits.push(edit(target, start, start + marker.length, Buffer.alloc(0)));
  }
  if (scenario === 'delete_source_preamble') {
    const marker = Buffer.from('#![allow(dead_code)]\n', 'utf8');
    const start = source.bytes.indexOf(marker);
    if (start < 0 || start !== source.bytes.lastIndexOf(marker)) invalid('source preamble deletion requires one exact inner-attribute marker');
    if (start < selected.end && start + marker.length > selected.start) invalid('source preamble must be outside the selected declaration');
    edits.push(edit(source, start, start + marker.length, Buffer.alloc(0)));
  }
  if (scenario === 'delete_outer_attachment' || scenario === 'transfer_outer_attachment') {
    const marker = Buffer.from('/// Sentinel docs\n#[inline]\n', 'utf8');
    const start = target.bytes.indexOf(marker);
    if (start < 0 || start !== target.bytes.lastIndexOf(marker)) invalid('attachment scenario requires one exact Sentinel attachment');
    if (scenario === 'delete_outer_attachment') {
      edits.push(edit(target, start, start + marker.length, Buffer.alloc(0)));
    } else {
      const ownerStart = start + marker.length;
      const ownerPrefix = Buffer.from('pub fn sentinel()', 'utf8');
      if (!target.bytes.subarray(ownerStart, ownerStart + ownerPrefix.length).equals(ownerPrefix)) invalid('attachment transfer requires the original sentinel owner');
      edits.push(edit(target, ownerStart, ownerStart, Buffer.from('pub fn stolen() {}\n', 'utf8')));
    }
  }
  if (scenario === 'transfer_parent_attachment') {
    if (destination.kind !== 'parentSymbol') invalid('parent attachment transfer requires a parentSymbol destination');
    const marker = Buffer.from('/// Bucket docs\n#[allow(dead_code)]\n', 'utf8');
    const start = target.bytes.indexOf(marker);
    const parent = sourceInterval(target, destinationBinding);
    if (start < 0 || start + marker.length !== parent.start) invalid('parent attachment transfer requires the exact attached Bucket corpus');
    edits.push(edit(target, parent.start, parent.start, Buffer.from('pub fn interceptor() {}\n', 'utf8')));
  }
  if (scenario === 'transfer_source_attachment') {
    const marker = Buffer.from('/// Remaining docs\n#[inline]\n', 'utf8');
    const start = source.bytes.indexOf(marker);
    if (start < 0 || start !== source.bytes.lastIndexOf(marker)) invalid('source attachment transfer requires one exact Remaining attachment');
    const ownerStart = start + marker.length;
    const ownerPrefix = Buffer.from('pub fn remaining()', 'utf8');
    if (!source.bytes.subarray(ownerStart, ownerStart + ownerPrefix.length).equals(ownerPrefix)) invalid('source attachment transfer requires the original remaining owner');
    edits.push(edit(source, ownerStart, ownerStart, Buffer.from('pub fn stolen_source() {}\n', 'utf8')));
  }
  const result = { schemaVersion: 1, requestId, edit: workspaceEdit(edits) };
  if (scenario === 'virtual') {
    result.edit.documentChanges.push({
      textDocument: { uri: target.uri, version: (target.version ?? 1) + 1 },
      edits: [{ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, newText: '// virtual phase\n' }],
    });
  }
  if (scenario === 'resource') result.edit.documentChanges.push({ kind: 'create', uri: new URL('fixture-created.rs', target.uri).href });
  if (scenario === 'resource_noop') result.edit.documentChanges.push({ kind: 'create', uri: target.uri, options: { ignoreIfExists: true } });
  if (scenario === 'repeated_original') result.edit.documentChanges.push({ textDocument: { uri: target.uri, version: null }, edits: [] });
  if (scenario === 'wrong_request_id') result.requestId = `${requestId}:wrong`;
  if (scenario === 'wrong_schema') result.schemaVersion = 2;
  if (scenario === 'extra_response_field') result.extra = 'must be rejected';
  return result;
}

function send(message) {
  const body = Buffer.from(JSON.stringify(message), 'utf8');
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function rpcError(id, code, message) {
  send({ jsonrpc: '2.0', id, error: { code, message } });
}

function dispatch(message) {
  const hasId = object(message) && Object.hasOwn(message, 'id');
  const id = hasId ? message.id : null;
  const validId = !hasId || id === null || typeof id === 'string' || (typeof id === 'number' && Number.isFinite(id));
  if (!object(message) || message.jsonrpc !== '2.0' || typeof message.method !== 'string' || !validId) {
    rpcError(null, -32600, 'Invalid Request');
    return;
  }
  // Include unexpected executeCommand calls so a host acceptance check can detect them.
  appendFileSync(transcriptPath, `${JSON.stringify({ method: message.method, params: message.params ?? null })}\n`, 'utf8');
  try {
    let result;
    if (message.method === 'initialize') {
      if (initialized) invalid('initialize may be called only once');
      const rootUri = message.params?.rootUri ?? message.params?.workspaceFolders?.[0]?.uri;
      if (rootUri != null) {
        try {
          const rootUrl = new URL(rootUri);
          if (rootUrl.protocol !== 'file:' || rootUrl.search || rootUrl.hash || (rootUrl.hostname && rootUrl.hostname.toLowerCase() !== 'localhost')) invalid('initialize root must be a plain local file URI');
          rootPath = path.resolve(fileURLToPath(rootUrl));
        }
        catch { invalid('initialize root must be a local file URI'); }
      }
      initialized = true;
      const capabilities = { positionEncoding: 'utf-16', textDocumentSync: 1 };
      if (scenario !== 'missing_capability') {
        capabilities.experimental = { yokeRefactorIntent: {
          schemaVersion: scenario === 'bad_capability_version' ? 2 : 1,
          operations: ['moveSymbol'], destinations: ['fileTopLevel', 'parentSymbol'],
        } };
        if (scenario !== 'missing_attachment_capability') {
          capabilities.experimental.yokeRefactorIntent.sourceAttachments =
            scenario === 'bad_attachment_capability' ? 'unsupported' : 'exact_text_v1';
        }
      }
      result = { capabilities, serverInfo: { name: 'independently-owned-own-v1-fixture', version: 'staging-only' } };
    } else if (message.method === 'exit') {
      process.exit(shutdown ? 0 : 1);
    } else if (!initialized) {
      const error = new Error('Server not initialized');
      error.rpcCode = -32002;
      throw error;
    } else if (message.method === 'shutdown') {
      shutdown = true;
      result = null;
    } else if (message.method === 'initialized' || message.method === '$/cancelRequest') {
      return;
    } else if (message.method === 'textDocument/didOpen') {
      const document = message.params?.textDocument;
      if (!object(document) || typeof document.uri !== 'string' || typeof document.text !== 'string' || !Number.isInteger(document.version)) invalid('invalid didOpen document');
      documents.set(document.uri, { text: document.text, version: document.version });
      return;
    } else if (message.method === 'textDocument/didChange') {
      const document = message.params?.textDocument;
      const changes = message.params?.contentChanges;
      if (!object(document) || !documents.has(document.uri) || !Number.isInteger(document.version) || !Array.isArray(changes) || changes.length !== 1 || typeof changes[0]?.text !== 'string' || Object.hasOwn(changes[0], 'range')) invalid('fixture supports full text synchronization only');
      documents.set(document.uri, { text: changes[0].text, version: document.version });
      return;
    } else if (message.method === 'textDocument/didClose') {
      documents.delete(message.params?.textDocument?.uri);
      return;
    } else if (message.method === 'yoke/refactorIntent') {
      if (shutdown) invalid('server is shut down');
      result = refactorIntent(message.params);
      appendFileSync(transcriptPath, `${JSON.stringify({
        event: 'fixtureOwnV1Result', scenario,
        kind: Object.hasOwn(result, 'edit') ? 'edit' : Object.hasOwn(result, 'unsupported') ? 'unsupported' : 'other',
        operationKinds: result.edit?.documentChanges?.map(change => change.kind ?? 'text') ?? [],
        textEditCounts: result.edit?.documentChanges?.map(change => change.edits?.length ?? 0) ?? [],
      })}\n`, 'utf8');
    } else {
      if (hasId) rpcError(id, -32601, 'Method not found');
      return;
    }
    if (hasId) send({ jsonrpc: '2.0', id, result });
  } catch (error) {
    if (hasId) rpcError(id, error.rpcCode ?? -32603, error.rpcCode ? error.message : 'Fixture internal error');
  }
}

function failFraming(message) {
  framingFailed = true;
  process.stdin.pause();
  process.exitCode = 1;
  rpcError(null, -32700, message);
}

process.stdin.on('data', chunk => {
  if (framingFailed) return;
  input = Buffer.concat([input, chunk]);
  while (input.length) {
    const headerEnd = input.indexOf('\r\n\r\n');
    if (headerEnd < 0) {
      if (input.length > MAX_HEADER_BYTES) failFraming('Fixture header limit exceeded');
      return;
    }
    if (headerEnd > MAX_HEADER_BYTES) return failFraming('Fixture header limit exceeded');
    const headers = input.subarray(0, headerEnd).toString('ascii').split('\r\n');
    const lengths = headers.filter(header => /^content-length:/i.test(header));
    if (lengths.length !== 1 || !/^content-length:[ \t]*[0-9]+[ \t]*$/i.test(lengths[0])) return failFraming('Invalid Content-Length');
    const length = Number(lengths[0].slice(lengths[0].indexOf(':') + 1).trim());
    if (!Number.isSafeInteger(length) || length <= 0 || length > MAX_MESSAGE_BYTES) return failFraming('Fixture message limit exceeded');
    const bodyStart = headerEnd + 4;
    if (input.length < bodyStart + length) return;
    const body = input.subarray(bodyStart, bodyStart + length);
    input = input.subarray(bodyStart + length);
    let message;
    try { message = JSON.parse(utf8.decode(body)); }
    catch { rpcError(null, -32700, 'Parse error'); continue; }
    try { dispatch(message); }
    catch {
      process.stderr.write('Fixture transcript write failed\n');
      process.exitCode = 1;
      process.stdin.pause();
      framingFailed = true;
      return;
    }
  }
});

process.stdin.on('end', () => {
  if (!framingFailed && input.length !== 0) failFraming('Incomplete fixture frame');
});
