# ProviderDeck

ProviderDeck merges official and OpenAI-compatible proxy models into the native ChatGPT/Codex model picker on macOS. Existing threads switch providers at turn boundaries through app-server `thread/unsubscribe` and `thread/resume` requests.

Official OpenAI traffic never passes through the ProviderDeck helper. ProviderDeck does not scan, rewrite, or back up Codex history, and it leaves Plugins, Skills, and MCP management to ChatGPT/Codex.

ProviderDeck is independent from CodexMate. It uses a separate application name, bundle identifier, ports, state directory, and update feed. See [README.md](README.md) for development commands and detailed design boundaries.
