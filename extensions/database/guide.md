# Database

Database keeps your connections to PostgreSQL, MySQL, SQLite, Redis and other
stores, lets you browse and read them, and runs changes only after a preview
you confirm. Passwords live in the desktop keyring, never in a command line.

## Commands

- `vr database connections list|get|add|policy|remove` manage saved
  connections. `add --password-stdin` reads the password from stdin.
- `vr database test`, `capabilities`, `browse`, `read` and `query` only read.
- `vr database saved-queries list|save|get|remove` keep queries; `save` reads
  the text from stdin when it is not given.
- `vr database mutations preview` describes a change and issues a token;
  `mutations apply` makes it with that token and the confirmation text.
- `vr database operations` lists what Veronica has done, newest first.

Every command accepts `--json`.
