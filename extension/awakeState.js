export const AWAKE_SWITCHES = [
    { key: 'preventSleep', untilKey: 'preventSleepUntil', command: 'keep-awake', title: 'Keep Awake' },
    { key: 'lidAwakeEnabled', untilKey: 'lidAwakeUntil', command: 'lid-awake', title: 'Lid Awake' },
];

export function awakeState(values, awakeSwitch, nowMs) {
    const enabled = values?.[awakeSwitch.key] === true;
    const raw = values?.[awakeSwitch.untilKey];
    const untilMs = Number.isInteger(raw) && raw > 0 ? raw : null;
    const active = enabled && (untilMs === null || untilMs > nowMs);
    return {
        enabled,
        untilMs,
        active,
        expired: enabled && untilMs !== null && untilMs <= nowMs,
    };
}

export function nextDeadline(values, nowMs) {
    let next = null;
    for (const awakeSwitch of AWAKE_SWITCHES) {
        const state = awakeState(values, awakeSwitch, nowMs);
        if (state.active && state.untilMs !== null) {
            next = next === null ? state.untilMs : Math.min(next, state.untilMs);
        }
    }
    return next;
}
