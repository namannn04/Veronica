export function parseCpuTicks(stat) {
    const line = stat?.split('\n').find(row => row.startsWith('cpu '));
    if (!line)
        return null;
    const fields = line.trim().split(/\s+/).slice(1).map(Number);
    if (fields.length < 5 || fields.some(value => !Number.isFinite(value)))
        return null;
    const total = fields.slice(0, 8).reduce((sum, value) => sum + value, 0);
    const idle = fields[3] + fields[4];
    return { total, idle };
}

export function cpuPercent(previous, current) {
    if (!previous || !current)
        return null;
    const total = current.total - previous.total;
    const idle = current.idle - previous.idle;
    if (total <= 0 || idle < 0)
        return null;
    return Math.min(100, Math.max(0, ((total - idle) / total) * 100));
}

export function memoryPercent(meminfo) {
    if (!meminfo)
        return null;
    const field = name => {
        const match = meminfo.match(new RegExp(`^${name}:\\s+(\\d+)`, 'm'));
        return match ? Number(match[1]) : null;
    };
    const total = field('MemTotal');
    const available = field('MemAvailable') ?? field('MemFree');
    if (!total || total <= 0 || available === null)
        return null;
    return Math.min(100, Math.max(0, ((total - available) / total) * 100));
}
