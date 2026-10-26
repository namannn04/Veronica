/* Presenter mode as the notch reads it from the settings file.
 *
 * Free of gi:// imports so it runs under Node. It mirrors
 * `veronica_core::presenter::PresenterState`: the feature must be enabled, and
 * either the user or screen-share detection must have asked for it; a dismissed
 * detection does not count. The notch, the app and `vr presenter status` all
 * read one file, and they must reach the same answer from it.
 */

/** Settings keys of the categories the notch shows. */
export const BLUR_KEYS = {
    money: 'presenterBlurMoney',
    usage: 'presenterBlurUsage',
    agents: 'presenterBlurAgents',
    music: 'presenterBlurMusic',
};

function flag(values, key, fallback) {
    const value = values?.[key];
    return typeof value === 'boolean' ? value : fallback;
}

export function presenterState(values) {
    const enabled = flag(values, 'presenterEnabled', false);
    const manual = enabled && flag(values, 'presenterMode', false);
    const autoEffective = enabled &&
        flag(values, 'presenterAutoActive', false) &&
        flag(values, 'presenterAutoEnabled', true) &&
        !flag(values, 'presenterAutoPaused', false);
    const active = manual || autoEffective;
    const blurs = key => active && flag(values, key, true);
    return {
        enabled,
        manual,
        autoEffective,
        active,
        // The usage card carries spend, token figures and the rate-limit rings.
        usagePrivate: blurs(BLUR_KEYS.money) || blurs(BLUR_KEYS.usage) || blurs(BLUR_KEYS.agents),
        musicPrivate: blurs(BLUR_KEYS.music),
    };
}

/**
 * What a click on the notch's Presenter tile should ask `vr presenter` to do.
 *
 * Off, it starts. On, it undoes whatever is holding it on: the manual switch,
 * and a detected share for the rest of that share (as Edith's dismiss does).
 */
export function presenterClick(state) {
    if (!state.active)
        return ['start'];
    const commands = [];
    if (state.manual)
        commands.push('stop');
    if (state.autoEffective)
        commands.push('dismiss');
    return commands;
}
