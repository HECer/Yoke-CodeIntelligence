"""Opt-in Jedi static semantics. User code is never passed to Interpreter or executed.
Inputs and outputs are UTF-8 JSON. Locations are byte-exact and snapshot-relative.
Jedi's finite project search is not an exhaustive whole-program proof.
"""
from __future__ import annotations
import json, keyword, sys, pathlib, hashlib


def main():
    import jedi
    # Jedi 0.20 requires its parser cache for name lookup. Each request gets
    # a separate process, so no mutable parser state is shared across sessions.
    jedi.settings.fast_parser = True
    data = json.load(sys.stdin)
    root = pathlib.Path(data['root']).resolve()
    path = root.joinpath(data['path']).resolve()
    path.relative_to(root)
    files = data['files']
    if data['path'] not in files:
        raise ValueError('Input is not captured')
    source = path.read_text(encoding='utf-8')
    raw = source.encode('utf-8')
    pos = data['byte']
    prefix = raw[:pos].decode('utf-8')
    if pos < 0 or pos > len(raw):
        raise ValueError('Invalid byte offset')
    line = prefix.count('\n') + 1
    column = len(prefix.rsplit('\n', 1)[-1])
    project = jedi.Project(str(root), sys_path=[str(root), str(root / 'src')], smart_sys_path=False, load_unsafe_extensions=False)
    script = jedi.Script(source, path=str(path), project=project)
    operation = data['operation']
    result = {'provider': 'jedi', 'version': jedi.__version__, 'items': [], 'external': [], 'edits': [],
              'exhaustive': False, 'sourceExecuted': False,
              'limitations': ['Jedi resolves static Python names; dynamic imports, monkey-patching and bounded reference search may omit targets.']}
    if operation == 'definition':
        names = script.goto(line, column, follow_imports=True, follow_builtin_imports=False)
    elif operation == 'references':
        names = script.get_references(line, column, include_builtins=False, scope='project')
    elif operation == 'rename':
        new = data['newName']
        if not new.isidentifier() or keyword.iskeyword(new):
            raise ValueError('New name must be a Python identifier, not a keyword')
        # Fail closed on an existing binding rather than silently capturing references.
        import ast
        for relative in files:
            if not relative.endswith('.py'):
                continue
            tree = ast.parse(root.joinpath(relative).read_text(encoding='utf-8'))
            for node in ast.walk(tree):
                if (isinstance(node, ast.Name) and isinstance(node.ctx, (ast.Store, ast.Param)) and node.id == new) or (isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and node.name == new) or (isinstance(node, ast.arg) and node.arg == new) or (isinstance(node, ast.alias) and (node.asname or node.name.split('.')[0]) == new):
                    raise ValueError('Rename conflicts with an existing captured binding')
        changed = script.rename(line, column, new_name=new).get_changed_files()
        for p, value in changed.items():
            p = pathlib.Path(p).resolve()
            rel = p.relative_to(root).as_posix()
            if rel not in files:
                raise ValueError('Rename would modify an uncaptured file')
            text = value.get_new_code()
            # Syntax validation is not a claim of runtime equivalence.
            compile(text, str(p), 'exec', flags=__import__('ast').PyCF_ONLY_AST)
            result['edits'].append({'path': rel, 'text': text})
        names = []
    else:
        raise ValueError('Unsupported operation')
    for name in names:
        if name.module_path is None or name.line is None:
            result['external'].append({'name': name.full_name or name.name, 'reason': 'builtin_or_unlocated'})
            continue
        try:
            rel = pathlib.Path(name.module_path).resolve().relative_to(root).as_posix()
        except ValueError:
            result['external'].append({'name': name.full_name or name.name, 'reason': 'outside_snapshot'})
            continue
        if rel not in files:
            result['external'].append({'name': name.full_name or name.name, 'reason': 'uncaptured'})
            continue
        content = root.joinpath(rel).read_text(encoding='utf-8')
        lines = content.splitlines(keepends=True)
        begin = ''.join(lines[:name.line - 1]) + lines[name.line - 1][:name.column]
        start = len(begin.encode('utf-8'))
        end = start + len(name.name.encode('utf-8'))
        if content.encode('utf-8')[start:end].decode('utf-8') != name.name:
            raise ValueError('Jedi location cannot be grounded in original source')
        result['items'].append({'path': rel, 'name': name.name, 'startByte': start, 'endByte': end, 'startLine': name.line, 'endLine': name.line})
    print(json.dumps(result, ensure_ascii=True))

if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print(json.dumps({'error': type(exc).__name__, 'message': str(exc)[:2000]}))
        sys.exit(2)
