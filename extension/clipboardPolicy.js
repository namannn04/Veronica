const SECRET_HINTS = ['x-kde-passwordManagerHint'];

export function isSecretCopy(mimeTypes) {
    return Array.isArray(mimeTypes) &&
        mimeTypes.some(type => SECRET_HINTS.includes(type));
}
