'use strict';
const token = location.hash.slice(1);
history.replaceState(null, '', location.pathname);
const $ = id => document.getElementById(id);
let snapshotId, selected, requestSequence = 0;
const notice = message => { $('notice').textContent = message; };
async function call(tool, args = {}) {
    const response = await fetch('/api', { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ tool, arguments: { ...(snapshotId ? { snapshotId } : {}), ...args } }) });
    const data = await response.json();
    if (!response.ok)
        throw new Error(data.error || 'Request failed');
    if (data.status === 'error')
        throw new Error(data.error.message);
    if (data.snapshotId) {
        snapshotId = data.snapshotId;
        $('snapshot').textContent = snapshotId;
    }
    notice([...(data.warnings || []), data.artifactId ? `Result is partial; full artifact ${data.artifactId} is available through ci_read.` : ''].filter(Boolean).join(' '));
    return data;
}
async function refresh(build = false) {
    try {
        notice(build ? 'Indexing saved files…' : 'Reading status…');
        if (build)
            snapshotId = undefined;
        const response = await call('ci_status', build ? { mode: 'index', timeoutMs: 120000 } : {});
        const data = response.data, cov = data.coverage;
        $('coverage').textContent = cov ? `${cov.indexedFiles} indexed files · ${cov.unresolved} unresolved relationships` : 'Not indexed';
        if (!response.warnings.length)
            notice(build ? 'Index ready. Source files were not changed.' : '');
    }
    catch (error) {
        notice(error.message);
    }
}
function render(items) {
    $('results').replaceChildren();
    $('count').textContent = items.length;
    selected = undefined;
    $('trace').disabled = true;
    for (const item of items) {
        const entity = item.entity || item;
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'result';
        const name = document.createElement('code');
        name.textContent = entity.qualifiedName || entity.name || entity.path || 'Source result';
        const path = document.createElement('small');
        path.textContent = `${entity.kind || 'source'} · ${entity.path || ''}${entity.startLine ? ':' + entity.startLine : ''}`;
        button.append(name, path);
        button.addEventListener('click', async () => {
            try {
                document.querySelectorAll('.result.active').forEach(x => x.classList.remove('active'));
                button.classList.add('active');
                selected = entity;
                $('detail-title').textContent = name.textContent;
                $('trace').disabled = !entity.id;
                const result = await call('ci_read', entity.id ? { entityId: entity.id } : { path: entity.path, startByte: entity.startByte, endByte: entity.endByte });
                $('detail').textContent = result.data.text ?? JSON.stringify(result.data, null, 2);
            }
            catch (error) {
                notice(error.message);
            }
        });
        $('results').append(button);
    }
    if (!items.length) {
        const p = document.createElement('p');
        p.className = 'empty';
        p.textContent = 'No results in the examined scope. Check index coverage before assuming the code is unused.';
        $('results').append(p);
    }
}
$('search').addEventListener('submit', async (event) => {
    event.preventDefault();
    const sequence = ++requestSequence;
    try {
        notice('Searching…');
        const kind = $('kind').value;
        const response = await call(kind === 'context' ? 'ci_context' : 'ci_find', { query: $('query').value, ...(kind === 'context' ? {} : { kind }), limit: 20 });
        if (sequence !== requestSequence)
            return;
        render(response.data.items || []);
        if (kind === 'context')
            $('detail').textContent = JSON.stringify(response.data, null, 2);
    }
    catch (error) {
        notice(error.message);
    }
});
$('trace').addEventListener('click', async () => {
    try {
        if (selected?.id) {
            const result = await call('ci_trace', { entityIds: [selected.id], direction: 'both', depth: 2 });
            $('detail').textContent = JSON.stringify(result.data, null, 2);
        }
    }
    catch (error) {
        notice(error.message);
    }
});
$('index').addEventListener('click', () => refresh(true));
if (!token)
    notice('Open the authenticated local URL printed by code-intelligence view.');
else
    void refresh();
