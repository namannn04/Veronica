/* Which clipboard contents the history must not keep.
 *
 * Free of gi:// imports so it runs under Node. Password managers mark a copied
 * secret with `x-kde-passwordManagerHint` (KeePassXC, KWallet and others use
 * KDE's convention on every desktop) precisely so clipboard managers leave it
 * out; recording it would put the password in a file on disk.
 */

const SECRET_HINTS = ['x-kde-passwordManagerHint'];

/** Whether a copy offered with these MIME types is a secret. */
export function isSecretCopy(mimeTypes) {
    return Array.isArray(mimeTypes) &&
        mimeTypes.some(type => SECRET_HINTS.includes(type));
}
