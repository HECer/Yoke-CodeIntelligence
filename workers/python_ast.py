#!/usr/bin/env python3
"""Bounded, stdlib-only Python AST extractor. No project imports or execution.

Python AST column offsets are UTF-8 byte offsets. These are retained rather
than accidentally reinterpreted as Unicode code-point or UTF-16 offsets.
"""
import ast
import json
import sys

PRODUCER = 'python-ast@' + '.'.join(map(str, sys.version_info[:3]))

def extract(path, content):
    raw = content.encode('utf-8')
    lines = raw.splitlines(keepends=True)
    starts = [0]
    for line in lines:
        starts.append(starts[-1] + len(line))
    def span(node):
        sl = getattr(node, 'lineno', 1)
        el = getattr(node, 'end_lineno', sl) or sl
        return dict(startByte=starts[sl-1] + getattr(node, 'col_offset', 0),
                    endByte=starts[el-1] + (getattr(node, 'end_col_offset', 0) or 0),
                    startLine=sl, endLine=el)
    entities, edges, diagnostics = [], [], []
    try:
        tree = ast.parse(content, filename=path, type_comments=True)
    except (SyntaxError, ValueError, RecursionError) as exc:
        return dict(entities=[], observations=[], diagnostics=[dict(path=path, code='PY_SYNTAX', severity='error', message=str(exc))], limitations=[])
    names = {}
    def collect(body, parent=None):
        for node in body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                r = span(node)
                kind = 'class' if isinstance(node, ast.ClassDef) else 'function'
                key = f'{path}#{r["startByte"]}:{kind}:{node.name}'
                qualified = parent['qualifiedName'] + '.' + node.name if parent else node.name
                name_bytes = node.name.encode('utf-8')
                first_line = lines[node.lineno-1]
                search_start = node.col_offset + (6 if isinstance(node, ast.ClassDef) else 10 if isinstance(node, ast.AsyncFunctionDef) else 4)
                name_offset = first_line.find(name_bytes, search_start)
                item = dict(key=key, path=path, name=node.name, qualifiedName=qualified,
                            kind=kind, language='python', signature=first_line.decode('utf-8').strip()[:400],
                            producer=PRODUCER, parentKey=parent['key'] if parent else 'file:' + path, **r)
                if name_offset >= 0:
                    item['nameStartByte'] = starts[node.lineno-1] + name_offset
                    item['nameEndByte'] = item['nameStartByte'] + len(name_bytes)
                entities.append(item)
                names[id(node)] = item
                edges.append(dict(sourceKey=item['parentKey'], targetKey=key, relation='contains', resolution='syntactic', path=path, producer=PRODUCER, **r))
                collect(node.body, item)
            else:
                # Collect definitions nested in control-flow blocks without treating
                # the block as a lexical namespace.
                for field in ('body', 'orelse', 'finalbody'):
                    children = getattr(node, field, None)
                    if isinstance(children, list): collect(children, parent)
                for handler in getattr(node, 'handlers', []): collect(handler.body, parent)
    collect(tree.body)
    by_name = {}
    for item in entities: by_name.setdefault(item['qualifiedName'], []).append(item)
    class Walker(ast.NodeVisitor):
        def __init__(self): self.owner = 'file:' + path; self.prefix = ''
        def scoped(self, node):
            old_owner, old_prefix = self.owner, self.prefix
            entry = names.get(id(node))
            if entry: self.owner, self.prefix = entry['key'], entry['qualifiedName']
            if isinstance(node, ast.ClassDef):
                for base in node.bases:
                    label = ast.unparse(base)
                    edges.append(dict(sourceKey=self.owner, targetLabel=label, relation='inherits', resolution='unresolved', path=path, producer=PRODUCER, **span(base)))
            self.generic_visit(node)
            self.owner, self.prefix = old_owner, old_prefix
        visit_FunctionDef = scoped
        visit_AsyncFunctionDef = scoped
        visit_ClassDef = scoped
        def visit_Call(self, node):
            label = ast.unparse(node.func)
            # Name-only resolution is deliberately a heuristic: Python rebinding,
            # globals, descriptors, monkey-patching and dynamic imports may differ.
            match = None
            if isinstance(node.func, ast.Name):
                scopes = self.prefix.split('.') if self.prefix else []
                for n in range(len(scopes), -1, -1):
                    q = '.'.join(scopes[:n] + [node.func.id])
                    found = by_name.get(q, [])
                    if len(found) == 1: match = found[0]; break
            item = dict(sourceKey=self.owner, relation='calls', resolution='heuristic' if match else 'unresolved', path=path, producer=PRODUCER, **span(node.func))
            if match: item['targetKey'] = match['key']
            else: item['targetLabel'] = label
            edges.append(item)
            self.generic_visit(node)
        def visit_Import(self, node):
            for name in node.names:
                edges.append(dict(sourceKey='file:' + path, targetLabel=name.name, relation='imports', resolution='unresolved', path=path, producer=PRODUCER, **span(node)))
        def visit_ImportFrom(self, node):
            label = '.' * node.level + (node.module or '')
            edges.append(dict(sourceKey='file:' + path, targetLabel=label, relation='imports', resolution='unresolved', path=path, producer=PRODUCER, **span(node)))
    Walker().visit(tree)
    return dict(entities=entities, observations=edges, diagnostics=diagnostics,
                limitations=['Python calls are syntax candidates, not runtime or type-checker proofs.'])

def main():
    payload = json.loads(sys.stdin.buffer.read().decode('utf-8'))
    results = []
    for file in payload['files']:
        results.append(extract(file['path'], file['content']))
    sys.stdout.buffer.write(json.dumps(dict(results=results, producer=PRODUCER), ensure_ascii=False).encode('utf-8'))

if __name__ == '__main__':
    main()
