/** Preserve per-file Node test summaries: a file wrapper passing is not a test. */
export default async function* report(events) {
    for await (const event of events) {
        if (event.type === 'test:summary' || event.type === 'test:fail' || event.type === 'test:stderr') {
            yield JSON.stringify(event, (_key, value) => value instanceof Error ? { name: value.name, message: value.message, stack: value.stack } : value) + '\n';
        }
    }
}
