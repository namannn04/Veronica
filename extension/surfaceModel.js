export const TEMPLATES = ['ring', 'meter', 'stat', 'text', 'list', 'nowPlaying', 'actions'];
export const HIDDEN = '•••';
const MAX_ACTIVITIES = 4;
const MAX_ITEMS = 5;

const PRIVATE_CATEGORIES = {
    music: 'musicPrivate',
    money: 'usagePrivate',
    usage: 'usagePrivate',
    agents: 'usagePrivate',
};

function text(value) {
    return typeof value === 'string' ? value : '';
}

function isSurface(value) {
    return (
        value !== null &&
        typeof value === 'object' &&
        typeof value.extension === 'string' &&
        typeof value.id === 'string' &&
        TEMPLATES.includes(value.template) &&
        value.state !== null &&
        typeof value.state === 'object'
    );
}

export function parseSurfaces(raw) {
    let parsed = raw;
    if (typeof raw === 'string') {
        try {
            parsed = JSON.parse(raw);
        } catch (_error) {
            return [];
        }
    }
    return Array.isArray(parsed) ? parsed.filter(isSurface) : [];
}

export function notchActivities(surfaces) {
    return surfaces
        .filter(surface => surface.placement === 'notch')
        .sort(
            (a, b) =>
                (b.priority ?? 0) - (a.priority ?? 0) ||
                a.extension.localeCompare(b.extension) ||
                a.id.localeCompare(b.id)
        )
        .slice(0, MAX_ACTIVITIES);
}

export function isPrivate(surface, presenter) {
    const flag = PRIVATE_CATEGORIES[surface.blurCategory];
    return Boolean(flag && presenter?.[flag]);
}

function fraction(value) {
    const number = Number(value);
    return Number.isFinite(number) ? Math.max(0, Math.min(1, number)) : 0;
}

export function surfaceView(surface, presenter = null) {
    const state = surface.state ?? {};
    const hidden = isPrivate(surface, presenter);
    const mask = value => (hidden ? HIDDEN : value);
    const title = text(surface.title) || text(state.label) || surface.id;
    const actions = (Array.isArray(surface.actions) ? surface.actions : [])
        .filter(action => Array.isArray(action?.run) && action.run.length > 0)
        .map(action => ({ id: action.id, label: text(action.label) || action.id, run: action.run }));
    const base = { key: `${surface.extension}/${surface.id}`, template: surface.template, hidden, actions };
    switch (surface.template) {
        case 'ring':
        case 'meter': {
            const value = hidden ? 0 : fraction(state.value);
            return {
                ...base,
                rows: [
                    {
                        kind: 'meter',
                        label: title,
                        fraction: value,
                        value: mask(text(state.caption) || `${Math.round(value * 100)}%`),
                    },
                ],
            };
        }
        case 'stat':
            return {
                ...base,
                rows: [
                    { kind: 'text', label: title, value: mask(String(state.value ?? '')) },
                    ...(text(state.caption) ? [{ kind: 'note', value: mask(state.caption) }] : []),
                ],
            };
        case 'text':
            return { ...base, rows: [{ kind: 'note', value: mask(text(state.text)) }] };
        case 'list':
            return {
                ...base,
                heading: title,
                rows: (Array.isArray(state.items) ? state.items : []).slice(0, MAX_ITEMS).map(item => ({
                    kind: 'text',
                    label: mask(text(item?.title)),
                    value: mask(text(item?.detail)),
                })),
            };
        case 'nowPlaying':
            return {
                ...base,
                rows: [
                    {
                        kind: 'text',
                        label: mask(text(state.title)),
                        value: mask([text(state.artist), text(state.album)].filter(Boolean).join(' · ')),
                    },
                ],
                playing: state.playing === true,
            };
        default:
            return { ...base, rows: text(state.caption) ? [{ kind: 'note', value: state.caption }] : [] };
    }
}

export function actionArgs(action) {
    return Array.isArray(action?.run) ? action.run.filter(part => typeof part === 'string') : [];
}
