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
    const autoEffective =
        enabled &&
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
        usagePrivate: blurs(BLUR_KEYS.money) || blurs(BLUR_KEYS.usage) || blurs(BLUR_KEYS.agents),
        musicPrivate: blurs(BLUR_KEYS.music),
    };
}

export function presenterClick(state) {
    if (!state.active) {
        return ['start'];
    }
    const commands = [];
    if (state.manual) {
        commands.push('stop');
    }
    if (state.autoEffective) {
        commands.push('dismiss');
    }
    return commands;
}
