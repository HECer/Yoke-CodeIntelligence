import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { webcrypto } from 'node:crypto';

// A labeled DOM/protocol fixture, not native-host or visual acceptance. Execute
// the shipped script; expose its state instead of starting polling or a server.
const html = readFileSync(new URL('../native/src/session_dashboard.html', import.meta.url), 'utf8');
const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];
assert.equal(scripts.length, 1, 'one inline dashboard script is expected');
const startup = 'refresh();setInterval(refresh,1500);';
assert.equal(scripts[0][1].split(startup).length, 2, 'startup bridge must replace exactly one polling call');
const source = scripts[0][1].replace(startup, `
globalThis.dashboardFixture = {
 renderDocuments, pendingDocuments,
 install(value, selected) { data=value; fileEditor=selected; },
 changeBinding(project, revision) { data.view.activeProject=project; data.view.management.sessionStateSha256=revision; },
 editor() { return fileEditor; }, recovery() { return fileRecovery; }
};`);

class Element {
    constructor(tagName = 'div') {
        this.tagName = tagName.toLowerCase(); this.children = []; this.listeners = new Map();
        this.dataset = {}; this.style = {}; this.attributes = new Map(); this._text = ''; this._value = '';
        for (const key of ['disabled', 'readOnly', 'hidden', 'selected']) {
            let value = false;
            Object.defineProperty(this, key, { get: () => value, set: next => { value = Boolean(next); } });
        }
    }
    get value() { return this._value; }
    set value(value) { this._value = String(value); }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this._text = String(value); this.children = []; }
    set innerHTML(_value) { throw new Error('This fixture admits text rendering only'); }
    append(...children) {
        for (const child of children) {
            this.children.push(child);
            if (this.tagName === 'select' && child.tagName === 'option' && (!this._value || child.selected)) this._value = child.value;
        }
    }
    replaceChildren(...children) { this.children = []; this._text = ''; if (this.tagName === 'select') this._value = ''; this.append(...children); }
    get options() { return this.children.filter(child => child.tagName === 'option'); }
    get selectedOptions() { return this.options.filter(option => option.selected); }
    addEventListener(kind, listener) { const list = this.listeners.get(kind) || []; list.push(listener); this.listeners.set(kind, list); }
    removeEventListener(kind, listener) { this.listeners.set(kind, (this.listeners.get(kind) || []).filter(value => value !== listener)); }
    async dispatch(kind) { for (const listener of this.listeners.get(kind) || []) await listener({ currentTarget: this }); }
    setAttribute(key, value) { this.attributes.set(key, String(value)); }
    removeAttribute(key) { this.attributes.delete(key); }
    focus() {}
    querySelectorAll(selector) {
        assert.equal(selector, 'button', 'only recovery button traversal is required');
        const result = [];
        const visit = element => { for (const child of element.children) { if (child.tagName === 'button') result.push(child); visit(child); } };
        visit(this); return result;
    }
    showModal() { throw new Error('A blocked action must not request confirmation'); }
}

const project = 'fixture-源';
const revision = 'a'.repeat(64);
const draft = '\ufefftitle: "änderung 源"\n# <script>inert draft</script>\n';
const original = '\ufefftitle: "original 源"\r\n';
const plan = () => ({ id: 'reviewed-plan', beforeHash: 'b'.repeat(64), afterHash: 'c'.repeat(64), content: draft.replaceAll('\n', '\r\n') });

function fixture(replies) {
    const nodes = new Map([...html.matchAll(/\bid="([^"]+)"/g)].map(match => [match[1], new Element(match[1].endsWith('-choice') ? 'select' : 'div')]));
    const node = id => { assert(nodes.has(id), `unknown dashboard element ${id}`); return nodes.get(id); };
    const requests = [], windowListeners = new Map();
    let responseIndex = 0;
    const context = {
        document: { getElementById: node, createElement: tag => new Element(tag), querySelectorAll: () => [] },
        window: { addEventListener: (kind, listener) => windowListeners.set(kind, listener) },
        location: { hash: '#' + 'd'.repeat(64), pathname: '/dashboard' },
        sessionStorage: { setItem() {}, getItem: () => null }, history: { replaceState() {} },
        crypto: webcrypto, AbortController, TextEncoder, Uint8Array, setTimeout, clearTimeout,
        setInterval: () => { throw new Error('Polling must not run in this fixture'); },
        fetch: async (url, options) => {
            assert.equal(url, '/api/manage');
            const request = JSON.parse(options.body); requests.push(request);
            assert.equal(request.action.kind, 'document');
            assert.equal(request.action.request.kind, 'pending', 'blocked controls must not send a mutation');
            assert(responseIndex < replies.length, 'unexpected management request');
            const response = replies[responseIndex++];
            const payload = typeof response === 'function' ? await response(request) : response;
            return { ok: true, status: 200, json: async () => ({ schema: 'native_dashboard_management_v1', id: request.id,
                outcome: 'completed', sessionStateSha256: request.sessionStateSha256, data: payload }) };
        }
    };
    vm.runInNewContext(source, context, { filename: 'session_dashboard.html', timeout: 1000 });
    const api = context.dashboardFixture;
    const reviewedPlan = plan();
    const editor = { id: 'global-config', binding: { project, sessionStateSha256: revision },
        original, shown: original.replaceAll('\r\n', '\n'), revision: 'saved-file-revision', dirty: true, uncertain: false, plan: reviewedPlan };
    api.install({ view: { activeProject: project, projects: [], modes: [], management: { enabled: true,
        sessionStateSha256: revision, configurationFiles: { choices: [
            { id: editor.id, label: 'Owned global configuration', scope: { kind: 'global' }, writable: true }
        ] } } } }, editor);
    node('document-choice').value = editor.id;
    node('document-content').value = draft;
    node('document-review').hidden = false;
    api.renderDocuments();
    return { api, node, requests, editor, reviewedPlan, windowListeners };
}

function retained(f) {
    assert.equal(f.node('document-content').value, draft, 'the Unicode draft is retained exactly');
    assert.equal(f.api.editor(), f.editor);
    assert.equal(f.editor.plan, f.reviewedPlan, 'inspection must preserve the reviewed plan');
    assert.equal(f.editor.dirty, true);
    assert.equal(f.node('document-review').hidden, false);
}

function locked(f) {
    assert.equal(f.node('document-content').readOnly, true);
    for (const id of ['plan-document', 'apply-document', 'check-document-approval']) assert.equal(f.node(id).disabled, true, id);
}

test('owner retirement keeps the reviewed draft locked and identifies the original host operation', { timeout: 3000 }, async () => {
    const f = fixture([{ journals: [], blockedByOwnerRetirement: true, complete: false }]);
    assert.equal(f.node('document-content').readOnly, false);
    assert.equal(f.node('apply-document').disabled, false);
    await f.api.pendingDocuments();
    retained(f); locked(f);
    assert.equal(f.api.recovery().complete, false);
    assert.equal(f.node('document-journals').hidden, false);
    assert.equal(f.node('document-journal-list').querySelectorAll('button').length, 0);
    assert.match(f.node('document-journal-list').textContent, /recovery list is incomplete/i);
    assert.match(f.node('document-journal-list').textContent, /cannot retire, recover or reactivate owners/i);
    const message = f.node('document-state').textContent;
    assert.match(message, /owner retirement/i);
    assert.match(message, /original host selection/i);
    assert.doesNotMatch(message, /pending journal|explicit recovery before another save/i);
    await f.node('plan-document').dispatch('click');
    await f.node('apply-document').dispatch('click');
    await f.node('check-document-approval').dispatch('click');
    assert.equal(f.requests.length, 1, 'disabled controls send no request');
    const unloading = { prevented: false, preventDefault() { this.prevented = true; } };
    f.windowListeners.get('beforeunload')(unloading);
    assert.equal(unloading.prevented, true, 'closing still warns about the retained dirty draft');
});

test('ordinary pending journal keeps its recovery directions and existing editor message', { timeout: 3000 }, async () => {
    const f = fixture([{ journals: [{ pending: true, plan: { id: 'journal-plan', content: 'recorded 源' },
        phase: 'prepared', eventId: 'event-1', allowedDirections: ['resume', 'rollback'] }] }]);
    await f.api.pendingDocuments();
    retained(f); locked(f);
    assert.equal(f.node('document-state').textContent, 'A pending journal requires explicit recovery before another save. Your draft is preserved.');
    const buttons = f.node('document-journal-list').querySelectorAll('button');
    assert.equal(buttons.length, 2);
    assert(buttons.every(button => !button.disabled), 'ordinary supported recovery remains available');
    assert.doesNotMatch(f.node('document-journal-list').textContent, /owner retirement/i);
});

test('a different host write selection retains its existing authority-specific refusal', { timeout: 3000 }, async () => {
    const f = fixture([{ journals: [], blockedByDifferentAuthority: true }]);
    await f.api.pendingDocuments();
    retained(f); locked(f);
    assert.equal(f.node('document-state').textContent, 'This file belongs to another host write selection. Restore the original selection and journal folder. Your draft is preserved.');
    assert.match(f.node('document-journal-list').textContent, /different host write selection/i);
    assert.equal(f.node('document-journal-list').querySelectorAll('button').length, 0);
});

test('fresh inspection after removal of a retirement blocker does not retain its stale message', { timeout: 3000 }, async () => {
    const f = fixture([{ journals: [], blockedByOwnerRetirement: true, complete: false }, { journals: [] }]);
    await f.api.pendingDocuments();
    locked(f);
    await f.api.pendingDocuments();
    retained(f);
    assert.equal(f.node('document-content').readOnly, false);
    assert.equal(f.node('plan-document').disabled, false);
    assert.equal(f.node('apply-document').disabled, false);
    assert.equal(f.node('document-state').textContent, 'Proposal ready for review. Saving affects the selected file only.');
    assert.equal(f.node('document-journal-list').textContent, 'No pending journal for this selected file.');
});

for (const boundary of ['project', 'session']) {
    test(`${boundary} change during inspection discards the old retirement reply and preserves the bound draft`, { timeout: 3000 }, async () => {
        let entered, respond;
        const started = new Promise(resolve => { entered = resolve; });
        const waiting = new Promise(resolve => { respond = resolve; });
        const f = fixture([async () => { entered(); return waiting; }]);
        const inspection = f.api.pendingDocuments();
        await started;
        f.api.changeBinding(boundary === 'project' ? 'another-项目' : project, boundary === 'session' ? 'e'.repeat(64) : revision);
        respond({ journals: [], blockedByOwnerRetirement: true, complete: false });
        await inspection;
        retained(f); locked(f);
        assert.equal(f.api.recovery(), null, 'a stale reply must not become current recovery state');
        assert.equal(f.editor.binding.project, project);
        assert.equal(f.editor.binding.sessionStateSha256, revision);
        assert.match(f.node('document-state').textContent, /project or Session changed/i);
        assert.doesNotMatch(f.node('document-state').textContent, /owner retirement/i);
    });
}
