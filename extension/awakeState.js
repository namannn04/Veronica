/* Keep Awake and Lid Awake as the notch reads them from the settings file.
 *
 * Free of gi:// imports so it runs under Node. It mirrors
 * `veronica_core::awake::AwakeState`: a switch is on while `enabled` is true
 * and its deadline, if it has one, is still ahead. The two must not drift — the
 * notch holds an inhibitor on this answer, and the app holds one on Rust's.
 */

export const AWAKE_SWITCHES = [
    { key: 'preventSleep', untilKey: 'preventSleepUntil', command: 'keep-awake', title: 'Keep Awake' },
    { key: 'lidAwakeEnabled', untilKey: 'lidAwakeUntil', command: 'lid-awake', title: 'Lid Awake' },
];

/** One switch's state at `nowMs`. */
export function awakeState(values, awakeSwitch, nowMs) {
    const enabled = values?.[awakeSwitch.key] === true;
    const raw = values?.[awakeSwitch.untilKey];
    // A non-positive or non-integer deadline is not a deadline, as in Rust.
    const untilMs = Number.isInteger(raw) && raw > 0 ? raw : null;
    const active = enabled && (untilMs === null || untilMs > nowMs);
    return {
        enabled,
        untilMs,
        active,
        expired: enabled && untilMs !== null && untilMs <= nowMs,
    };
}

/** The soonest deadline of any running timed session, or null. */
export function nextDeadline(values, nowMs) {
    let next = null;
    for (const awakeSwitch of AWAKE_SWITCHES) {
        const state = awakeState(values, awakeSwitch, nowMs);
        if (state.active && state.untilMs !== null)
            next = next === null ? state.untilMs : Math.min(next, state.untilMs);
    }
    return next;
}
