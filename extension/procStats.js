/* The procfs arithmetic behind the top-bar CPU and memory readout.
 *
 * Deliberately free of any `gi://` import, so it can be exercised without a
 * running shell — `node --test extension/` covers it. The same numbers are
 * computed by `vr system snapshot`, and both derive them the same way, so the
 * top bar and the System page cannot disagree by more than one sample.
 */

/**
 * Total and idle jiffies from the aggregate `cpu` line of /proc/stat.
 *
 * Idle counts both `idle` and `iowait`, which is what procfs consumers
 * conventionally treat as not-busy.
 *
 * Returns null for anything unparseable, so a caller keeps its last good
 * reading rather than showing a number it made up.
 */
export function parseCpuTicks(stat) {
    const line = stat?.split('\n').find(row => row.startsWith('cpu '));
    if (!line)
        return null;
    const fields = line.trim().split(/\s+/).slice(1).map(Number);
    if (fields.length < 5 || fields.some(value => !Number.isFinite(value)))
        return null;
    const total = fields.reduce((sum, value) => sum + value, 0);
    const idle = fields[3] + fields[4];
    return { total, idle };
}

/** Busy share between two readings, as a percentage, or null when undecidable. */
export function cpuPercent(previous, current) {
    if (!previous || !current)
        return null;
    const total = current.total - previous.total;
    const idle = current.idle - previous.idle;
    // A counter that did not move, or went backwards across a suspend, says
    // nothing about load. Reporting 0% there would read as an idle machine.
    if (total <= 0 || idle < 0)
        return null;
    return Math.min(100, Math.max(0, ((total - idle) / total) * 100));
}

/**
 * Used memory as a percentage of total, from /proc/meminfo.
 *
 * MemAvailable is the kernel's own estimate of what a new allocation could get,
 * which is what a person means by "free". MemFree alone counts the page cache as
 * used and reads alarmingly high on a perfectly healthy machine, so it is only
 * the fallback for a kernel too old to report MemAvailable.
 */
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
