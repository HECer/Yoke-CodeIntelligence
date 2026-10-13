import { assert } from '../core/util.js';
/** Small validator for the exact JSON Schema vocabulary used by our tools. */
export function validate(schema: unknown, value: unknown, path = 'arguments'): void {
    const s = schema as Record<string, any>;
    if (s.oneOf) {
        let matched = 0;
        for (const option of s.oneOf) {
            try {
                validate(option, value, path);
                matched++;
            }
            catch { }
        }
        assert(matched === 1, 'INVALID_ARGUMENT', `${path} must match exactly one supported shape`);
        return;
    }
    if (s.const !== undefined)
        assert(value === s.const, 'INVALID_ARGUMENT', `${path} must equal ${s.const}`);
    if (s.enum)
        assert(s.enum.includes(value), 'INVALID_ARGUMENT', `${path} is not an allowed value`);
    if (s.type === 'object') {
        assert(value !== null && typeof value === 'object' && !Array.isArray(value), 'INVALID_ARGUMENT', `${path} must be an object`);
        const o = value as Record<string, unknown>;
        for (const key of s.required ?? [])
            assert(Object.hasOwn(o, key), 'INVALID_ARGUMENT', `${path}.${key} is required`);
        for (const [key, v] of Object.entries(o)) {
            assert(s.additionalProperties !== false || Object.hasOwn(s.properties ?? {}, key), 'INVALID_ARGUMENT', `${path}.${key} is not supported`);
            if (s.properties?.[key])
                validate(s.properties[key], v, `${path}.${key}`);
        }
    }
    else if (s.type === 'array') {
        assert(Array.isArray(value), 'INVALID_ARGUMENT', `${path} must be an array`);
        assert(value.length >= (s.minItems ?? 0) && value.length <= (s.maxItems ?? 100000), 'INVALID_ARGUMENT', `${path} array size is invalid`);
        for (let i = 0; i < value.length; i++)
            validate(s.items, value[i], `${path}[${i}]`);
    }
    else if (s.type === 'string') {
        assert(typeof value === 'string' && value.length <= (s.maxLength ?? 8000000), 'INVALID_ARGUMENT', `${path} must be a bounded string`);
        if (s.pattern)
            assert(new RegExp(s.pattern).test(value), 'INVALID_ARGUMENT', `${path} has invalid format`);
    }
    else if (s.type === 'boolean')
        assert(typeof value === 'boolean', 'INVALID_ARGUMENT', `${path} must be boolean`);
    else if (s.type === 'number' || s.type === 'integer') {
        assert(typeof value === 'number' && Number.isFinite(value) && (s.type !== 'integer' || Number.isSafeInteger(value)), 'INVALID_ARGUMENT', `${path} must be ${s.type}`);
        assert(value >= (s.minimum ?? -Infinity) && value <= (s.maximum ?? Infinity), 'INVALID_ARGUMENT', `${path} is outside bounds`);
    }
}
