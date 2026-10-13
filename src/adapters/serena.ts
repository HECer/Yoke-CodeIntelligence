import { RpcPeer } from './rpc.js';
import { Deadline, object, assert } from '../core/util.js';
import { normalizeSerenaSymbols } from './graph-import.js';
/** Optional external, separately installed Serena MCP. Never imports its GPL application. */
export class SerenaAdapter {
    readonly peer = new RpcPeer('ndjson');
    private tools = new Set<string>();
    async start(command: string, args: string[], sandbox: string, deadline: Deadline): Promise<void> {
        this.peer.start(command, args.map(a => a.replaceAll('{workspace}', sandbox)), sandbox);
        await this.peer.request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'code-intelligence', version: '0.1.0' } }, deadline);
        this.peer.notify('notifications/initialized', {});
        const result = object(await this.peer.request('tools/list', {}, deadline));
        assert(Array.isArray(result.tools), 'ADAPTER_CONTRACT', 'MCP tools/list returned no tools');
        for (const tool of result.tools)
            this.tools.add(String(object(tool).name));
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
