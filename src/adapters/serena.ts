import { VERSION } from '../core/types.js';
import { RpcPeer } from './rpc.js';
import { Deadline, object, assert } from '../core/util.js';
import { normalizeSerenaSymbols } from './graph-import.js';
/** Optional external, separately installed Serena MCP. Never imports its GPL application. */
export class SerenaAdapter {
    readonly peer = new RpcPeer('ndjson');
    private tools = new Set<string>();
    private descriptions = new Map<string, Record<string, unknown>>();
    async start(command: string, args: string[], sandbox: string, deadline: Deadline): Promise<void> {
        this.peer.start(command, args.map(a => a.replaceAll('{workspace}', sandbox)), sandbox);
        await this.peer.request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'code-intelligence', version: VERSION } }, deadline);
        this.peer.notify('notifications/initialized', {});
        const seen = new Set<string>();
        let cursor: string | undefined;
        for (let page = 0; page < 100; page++) {
            const result = object(await this.peer.request('tools/list', cursor ? { cursor } : {}, deadline));
            assert(Array.isArray(result.tools), 'ADAPTER_CONTRACT', 'MCP tools/list returned no tools');
            for (const raw of result.tools) {
                const tool = object(raw);
                assert(typeof tool.name === 'string' && tool.name.length <= 200 && !this.tools.has(tool.name), 'ADAPTER_CONTRACT', 'Duplicate or invalid MCP tool');
                this.tools.add(tool.name);
                this.descriptions.set(tool.name, tool);
            }
            if (!result.nextCursor)
                break;
            assert(typeof result.nextCursor === 'string' && !seen.has(result.nextCursor) && page < 99, 'ADAPTER_CONTRACT', 'MCP tool pagination cycles or exceeds limit');
            cursor = result.nextCursor;
            seen.add(cursor);
        }
    }
    capabilities(): Record<string, unknown>[] { return [...this.descriptions.values()]; }
    async call(name: string, args: Record<string, unknown>, deadline: Deadline, options: {
        allowMutation?: boolean;
        allowArbitraryCode?: boolean;
    } = {}): Promise<unknown> {
        assert(this.tools.has(name), 'CAPABILITY_UNAVAILABLE', 'Requested tool is not advertised by this installed Serena version');
        const reads = new Set(['find_symbol', 'get_symbols_overview', 'find_referencing_symbols', 'find_implementations', 'find_declaration', 'diagnostics_file', 'diagnostics_symbol', 'search_for_pattern', 'find_file', 'list_dir', 'read_file', 'read_memory', 'list_memories', 'get_current_config', 'check_onboarding_performed', 'get_type_hierarchy']);
        const edits = new Set(['rename_symbol', 'replace_symbol_body', 'insert_after_symbol', 'insert_before_symbol', 'safe_delete_symbol', 'replace_content', 'rename', 'move', 'inline', 'safe_delete', 'move_symbol', 'move_file', 'rename_file', 'jet_brains_move', 'jet_brains_rename', 'jet_brains_safe_delete']);
        assert(reads.has(name) || options.allowMutation && edits.has(name) || options.allowArbitraryCode, 'CAPABILITY_UNAVAILABLE', 'Tool needs explicit mutation or arbitrary-runtime permission');
        assert(Buffer.byteLength(JSON.stringify(args)) <= 1000000, 'CAPACITY_EXCEEDED', 'Serena arguments too large');
        const result = object(await this.peer.request('tools/call', { name, arguments: args }, deadline));
        assert(!result.isError, 'PROVIDER_ERROR', 'Serena reported an operation error');
        // Tool output is data, never inherited authority or permission.
        return result.structuredContent ?? result.content ?? result;
    }
    async symbols(pattern: string, path: string, deadline: Deadline): Promise<ReturnType<typeof normalizeSerenaSymbols>> {
        assert(this.tools.has('find_symbol'), 'CAPABILITY_UNAVAILABLE', 'Serena version does not expose find_symbol');
        const result = object(await this.peer.request('tools/call', { name: 'find_symbol', arguments: { name_path_pattern: pattern, relative_path: path, include_body: false, depth: 1, max_answer_chars: 100000 } }, deadline));
        assert(!result.isError, 'PROVIDER_ERROR', 'Serena reported an error');
        if (result.structuredContent)
            return normalizeSerenaSymbols(result.structuredContent);
        assert(Array.isArray(result.content), 'ADAPTER_CONTRACT', 'Serena result is not MCP content');
        const texts = result.content.filter((c: any) => c.type === 'text').map((c: any) => c.text);
        assert(texts.length === 1, 'ADAPTER_CONTRACT', 'Ambiguous Serena text result');
        return normalizeSerenaSymbols(texts[0]);
    }
    close(): void { this.peer.close(); }
}
