/** Canonical evidence contracts. Offsets are UTF-8 bytes, ranges are half-open. */
export const VERSION = '0.1.0';
export const SCHEMA = 1;
export type Resolution = 'syntactic' | 'exact_binding' | 'possible_dispatch' | 'heuristic' | 'inferred_concept' | 'observed_execution' | 'authored_contract' | 'unresolved';
export interface SourceRange {
    startByte: number;
    endByte: number;
    startLine: number;
    endLine: number;
}
export interface ManifestFile {
    path: string;
    hash: string;
    size: number;
    mode: number;
    language: string;
}
export interface Snapshot {
    id: string;
    workspaceId: string;
    projectId: string;
    configHash: string;
    parentId?: string;
    kind: 'disk' | 'derived';
    files: ManifestFile[];
    createdAt: string;
    omissions: {
        path: string;
        reason: string;
    }[];
    captureConsistency: 'stable-double-scan' | 'derived-from-immutable';
}
export interface Entity extends SourceRange {
    id: string;
    snapshotId: string;
    key: string;
    path: string;
    name: string;
    qualifiedName: string;
    kind: string;
    language: string;
    signature: string;
    nameStartByte?: number;
    nameEndByte?: number;
    parentKey?: string;
    producer: string;
    exported?: boolean;
    metadata?: Record<string, unknown>;
}
export interface Evidence extends SourceRange {
    id: string;
    snapshotId: string;
    path: string;
    contentHash: string;
    producer: string;
    anchor: 'exact' | 'enclosing_symbol' | 'file' | 'artifact';
    trust: 'source-data';
    artifactId?: string;
}
export interface Observation {
    id: string;
    snapshotId: string;
    source: string;
    target: string | null;
    targetLabel?: string;
    relation: string;
    resolution: Resolution;
    evidenceIds: string[];
    producer: string;
    scope: string;
}
export interface Coverage {
    indexedFiles: number;
    sourceFiles: number;
    languages: Record<string, {
        files: number;
        provider: string;
        semantic: boolean;
    }>;
    unresolved: number;
    omissions: number;
    diagnostics: number;
    exhaustive: false;
    limitations: string[];
}
export interface Graph {
    snapshotId: string;
    entities: Entity[];
    observations: Observation[];
    evidence: Evidence[];
    coverage: Coverage;
    diagnostics: Diagnostic[];
}
export interface Diagnostic {
    path: string;
    message: string;
    severity: 'error' | 'warning';
    code: string;
    startByte?: number;
}
export interface LocalEntity extends SourceRange {
    key: string;
    path: string;
    name: string;
    qualifiedName: string;
    kind: string;
    language: string;
    signature: string;
    producer: string;
    nameStartByte?: number;
    nameEndByte?: number;
    parentKey?: string;
    exported?: boolean;
    metadata?: Record<string, unknown>;
}
export interface LocalObservation extends SourceRange {
    sourceKey: string;
    targetKey?: string;
    targetLabel?: string;
    relation: string;
    resolution: Resolution;
    path: string;
    producer: string;
    anchor?: Evidence['anchor'];
}
export interface Extraction {
    entities: LocalEntity[];
    observations: LocalObservation[];
    diagnostics: Diagnostic[];
    limitations: string[];
}
export interface RequestOptions {
    snapshotId?: string;
    maxBytes?: number;
    tokenBudget?: number;
    timeoutMs?: number;
    taskId?: string;
    cursor?: string;
}
export interface Envelope {
    schemaVersion: number;
    requestId: string;
    snapshotId: string | null;
    status: 'ok' | 'partial' | 'error';
    data: Record<string, unknown>;
    warnings: string[];
    error?: {
        code: string;
        message: string;
    };
    metrics: {
        durationMs: number;
        serializedBytes: number;
        estimatedTokens: number;
        tokenCount: 'estimated';
        tokenMethod: 'utf8_bytes_div_3';
        modelCalls: number;
        localOnly: boolean;
    };
    artifactId?: string;
    nextCursor?: string;
}
export interface Config {
    schemaVersion: 1;
    exclude: string[];
    maxFileBytes: number;
    maxFiles: number;
    maxSnapshotBytes: number;
    python: string;
    pythonEnabled: boolean;
    captureAttempts: number;
    maxResultBytes: number;
    timeoutMs: number;
    serviceScopes: Record<string, string>;
    verify: Record<string, {
        command: string;
        args: string[];
        timeoutMs?: number;
    }>;
}
export type FileEdit = {
    kind: 'replace';
    path: string;
    expectedHash: string;
    startByte: number;
    endByte: number;
    text: string;
} | {
    kind: 'create';
    path: string;
    text: string;
} | {
    kind: 'delete';
    path: string;
    expectedHash: string;
};
export interface PlannedFile {
    path: string;
    beforeHash: string | null;
    afterHash: string | null;
    mode: number;
}
export interface ChangePlan {
    id: string;
    baseSnapshotId: string;
    resultSnapshotId: string;
    workspaceId: string;
    createdAt: string;
    expiresAt: string;
    files: PlannedFile[];
    diff: string;
    warnings: string[];
    testFilesChanged: string[];
    validation: 'not_run';
}
export interface ApplyReceipt {
    id: string;
    planId: string;
    idempotencyKey: string;
    mode: 'isolated' | 'workspace';
    status: 'applied' | 'conflict' | 'recovery_required' | 'rolled_back';
    snapshotId: string;
    changedPaths: string[];
    sandboxPath?: string;
    appliedPaths: string[];
    error?: string;
}
