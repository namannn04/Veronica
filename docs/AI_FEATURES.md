# Agents and Companion

These features live in the full app, not the notch. Neither starts a model or coding agent on app startup.

## Agents & Herdr

Choose Claude Code, Codex, or a custom executable profile, set a project directory, and start an embedded terminal. Authenticate with the provider's normal CLI flow. Profiles contain executable names and argument arrays, not shell scripts; do not put secrets in saved arguments. Edit/remove profiles from the provider controls. The resume option opens the CLI's own conversation picker. Provider permissions and billing still apply.

Owned terminals support interactive input, resizing, selection, individual close and Stop all. Closing the main window stops Veronica-owned terminals. At most four terminals run at once, each retaining 256 KiB of output and a bounded input queue. Terminal rendering and polling stop while hidden. Output is not saved by Veronica; the CLI may save its own history. Multiline paste requires confirmation, and terminal clipboard-write escape sequences are blocked.

External current-user Claude/Codex/OpenCode/Aider processes are observations. Their activity is marked unknown unless a supported status source reports it. Veronica does not read arbitrary process arguments or take control of unrelated terminals. Codex's optional shared agent center uses the CLI's existing backend: stopping its frontend does not promise to stop shared tasks.

Existing Herdr sessions retain their board and detail view. **View in app** attaches an existing pane after an explicit takeover confirmation; the Herdr backend's lifecycle remains external. Herdr must be installed and reachable for this integration. Managed CLI terminals work without Herdr.

**Connect Claude status** explicitly installs a small local hook and merges only Veronica's hook groups into Claude settings. It reports last-known working/blocked/done/idle status for new sessions. Events contain status and timestamps keyed to process identity, never prompts, tool input, or transcripts. Disconnect removes only Veronica's entries. A last-reported state is not proof of current activity.

Native Debian/AppImage builds can run host CLIs. Flatpak cannot start managed host terminals; use a native package for this feature. Discoverability depends on executable paths and provider versions. Arbitrary desktop-app conversations cannot universally be resumed or controlled.

## Companion AI conversation

Open Companion → AI conversation. Configure Ollama, OpenAI Responses, an OpenAI-compatible API, or Anthropic. Supply your own model and, where required, API key. Keys stay in this window's memory; they are not saved in chat/config/export files. Re-enter a key after reopening. Cloud services may charge according to your account.

Companion always opens **off**. Enable it explicitly for the current window, then send a message or save a private entry without requesting a reply. Merely opening the page, configuring a provider or enabling the toggle does not run inference. Leaving the AI tab or closing the window switches it off, cancels pending requests and stops the private local runtime. Provider-side processing may continue after a remote HTTP request is cancelled. Externally managed Ollama services remain under their owner's control.

### Local Ollama

Install Python 3 and Ollama separately. The managed option starts a private loopback server only when needed, with one loaded model, one parallel request, keep-alive zero, a 4,096-token context and bounded generation. Existing model caches can be selected to avoid duplicate downloads. A system cache must be readable; downloading needs a writable cache. Downloading a model is a separate confirmed action, can use gigabytes of disk, and is never automatic. No Docker is required.

The private server is supervised through an app-owned pipe so abrupt parent exit also triggers cleanup. Resource use depends on the chosen model and hardware; a 3B model still consumes substantial RAM during a response. There is no zero-RAM guarantee. Switch off to release Veronica's private model process. Flatpak cannot launch the managed host runtime; connect an already-running local/remote endpoint or use a native package.

### Memory and privacy

Conversation and explicitly saved preferences stay in the displayed local chat JSON file, under the app's data directory. Directories use owner-only permissions and files use mode 0600. This is not encrypted storage. Data is not uploaded by Veronica except the context you choose to send to a configured provider. HTTP is allowed only for loopback endpoints; remote providers require HTTPS and explicit consent.

Use **Preview exact context** before sending. History and saved memory can be excluded independently. Context uses bounded recent messages and relevant older entries, not your entire device. Existing voice notes, activity logs and agent transcripts are not silently imported. Saved preferences can be edited or forgotten; chat history can be cleared independently. Export writes a separate local JSON copy. Clearing data does not erase exports, filesystem backups, or a provider's copies.

Limits: 1,000 stored messages, 12 KiB per user entry, 32 KiB per reply, 12 KiB explicit memory, a 2 MiB chat document, and the latest 200 messages displayed. Export or clear history when full. Memory is context retrieval, not model training or automatic personality diagnosis. The companion offers supportive conversation, not professional therapy or medical care.

OpenAI Responses requests use `store: false`; each provider's own retention policy still applies. Remote redirects and environment proxies are disabled for these requests. Error messages do not include API keys or provider response bodies.

## Validation scope

Automated checks cover PTY input/resize/shutdown, process ownership, status-hook privacy, local file permissions, endpoint consent, bounded context, mocked cloud requests and cancellation. Browser integration exercises real Rust controls through an isolated test bridge. A real local Ollama response is tested separately against an existing model cache. Paid cloud credentials and every third-party CLI/Herdr environment are not exercised by these tests.
