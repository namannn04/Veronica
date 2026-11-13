# Developer Workspace and Graftcode

Developer Workspace is available in the desktop sidebar and Ctrl+K. Project
Doctor works without Graftcode. The optional Playground talks to a Gateway you
start yourself. Nothing starts at login or polls while this page is idle.

## Project Doctor

Select a project directory, optionally enter up to 16 expected IPv4 localhost
TCP ports and 16 exact local Docker container names, then choose Check project.
The same checks are available through:

```sh
vr workspace doctor --root /absolute/path/to/project --ports 3000,5432 --containers my-app-db --json
```

Checks inspect a fixed list of root manifests, never recurse, read .env files,
install dependencies, run project scripts, or change services. Regular-file reads
are capped at 256 KiB and reject symlinks/devices/FIFOs. A found manifest does not
prove dependencies are installed; an open TCP port does not prove application
health. Docker checks run only when names are supplied and stop waiting after
six seconds. Ports are IPv4 127.0.0.1; IPv6-only services can appear unavailable.

## Graftcode Playground

1. Start a trusted module using an official Graftcode Gateway. Node.js, Python,
   .NET, Java, PHP and Ruby are documented runtimes; Rust is not a documented
   direct Gateway runtime. Veronica's Rust/Tauri IPC remains unchanged.
2. Enter its loopback endpoint, such as `http://127.0.0.1:18772/mcp`.
3. Supply the session access token if the receiver requires one. The Playground
   sends it as a bearer header. When the advertised method has a `sessionToken`
   parameter and the arguments do not supply it, Veronica adds the access token
   to that parameter. This is necessary for the bundled Doctor receiver: the
   tested Gateway 1.5.0 MCP path did not propagate bearer headers into its Node
   request context. AI callers must provide `sessionToken` as a tool argument.
4. Connect & discover, select a method, inspect its input schema, enter JSON
   arguments, then Review call and Confirm & run. Discovery executes no methods.
5. Copy a result or keep up to eight argument drafts for this session. Drafts,
   results and tokens are discarded when the endpoint/token is changed, the
   session is cleared or the Playground is closed. Avoid keeping credentials in
   argument drafts; the explicit `sessionToken` field cannot be saved.

Only HTTP loopback hosts are accepted. `localhost` is pinned to 127.0.0.1;
proxies, redirects, URL credentials/query/fragment and non-/mcp paths are refused.
There are at most two concurrent calls, 64 discovered tools, 16 KiB of arguments
(including injected token), 256 KiB per response, an eight-second HTTP timeout
and a twenty-second operation timeout. A method can keep running in an external
Gateway after a client timeout or page close. **Do not blindly retry a change.**
Sessions negotiate MCP, send initialized, validate the currently advertised tool,
and request session deletion afterwards, including ordinary failure paths.
Gateway-returned content and install commands are rendered as text, never HTML
or shell commands. Install commands can be stale or unusable when metadata
upload is disabled. They are displayed for review, never automatically run.

## Optional read-only Project Doctor receiver

The receiver is ordinary Node.js code hosted by the real Graftcode Gateway; it
is not a separate hand-written MCP server. Tested with Gateway **1.5.0** and Node
**22.22.1**. Gateway is a separate third-party install, not a mandatory Veronica
package dependency. Use official releases and inspect their license/setup:

- https://github.com/grft-dev/graftcode-gateway
- https://docs.graftcode.com/quick-start/expose-mcp/javascript

From a repository checkout (no npm install needed):

```sh
node integrations/graftcode/start.cjs --root /absolute/path/to/project --ports 3000,5432 --containers my-app-db
```

Optional flags: `--gg /absolute/path/to/gg`, `--vr /absolute/path/to/vr`,
`--port 18771` and `--http-port 18772`. Choose distinct unprivileged free ports.
The Debian package also installs the receiver at
`/usr/share/veronica/graftcode/start.cjs`. AppImage/Flatpak users can use the
repository receiver on the host alongside a host `vr`; there is no automatic
sandbox escape or bundled Node/Gateway runtime.

The foreground launcher preflights the selected project, creates a fresh random
256-bit access token, and prints the endpoint/token for your Playground. Keep
this token private. It launches Gateway at lower priority in a private temporary
working directory, disables model upload (GSMU) and exposes only `inspect` and
`attention`. Neither method accepts arbitrary filesystem paths or shell commands.
They execute only the fixed `vr workspace doctor` invocation for the chosen
project and omit the absolute root from successful replies. Generic failures
omit CLI diagnostics and paths. This is authenticated read-only access, not a
sandbox for untrusted project modules. Checks may expose project names, script
names and service/container availability to an authorized caller.

Gateway 1.5.0 has no documented interface-bind option and may listen on all
interfaces. Doctor methods require the token even from local callers; the
Gateway's public module/schema metadata can still be visible. Use a firewall or
network isolation when exposing any Gateway, and never expose other unguarded
methods. A local endpoint does not prove a module is trustworthy.

Ctrl+C stops the launcher's owned process group, with a two-second kill fallback,
and removes its own temporary directory. Closing Veronica's page does not stop
a separately started Gateway. No project or generated package is deleted.

## Generated Grafts and privacy

A Graft is the generated client package for a Gateway's selected public surface.
Vision also describes those loaded modules; it is not a graph of every process
or repository on your computer. To generate a usable package through Graftcode's
registry, model upload must be enabled intentionally for that module. The private
Doctor launcher disables it by default. Enabling upload shares callable-surface
metadata with Graftcode and can require project/registry configuration. Free
standalone registry identities may change on restart. Copy the exact commands
from the running Gateway, pin the generated package, and review its dependencies.
Do not put source secrets, tokens or project paths into public method signatures.

Verification used a separate, non-sensitive fixture with metadata upload enabled:
Gateway analysis -> actual registry-generated npm Graft -> installed temporary
client -> successful remote Project Doctor invocation. Both generated-Graft and
Gateway MCP paths rejected unauthorized tokens and returned real read-only CLI
checks for an authorized caller. No registry
package generation is performed during ordinary Veronica use or offline CI.

## Validation and resource bounds

Rust tests cover manifest/secret boundaries, symlinks/FIFOs/large files, real TCP
listeners, loopback validation, mandatory confirmation, redirect refusal, oversized
HTTP replies, session negotiation, token forwarding and DELETE cleanup. Node tests
verify unauthorized calls do not spawn `vr` and successful projections omit paths.
Interface tests cover bounded JSON, and browser verification exercised real CLI
Doctor/discovery/call flows with the actual Gateway. Optional native app checks
remain distinct from those browser flows; a browser IPC bridge is not Tauri itself.

The normal app has no Gateway process. Its existing polling behavior is unchanged;
only explicit workspace requests add work. A running Gateway/Node runtime does
consume additional memory and CPU, so enable it only when useful and stop it when
finished. No claim of zero overhead or universally faster execution is made.
