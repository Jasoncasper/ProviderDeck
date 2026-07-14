# ProviderDeck

ProviderDeck merges official and OpenAI-compatible proxy models into the native ChatGPT/Codex model picker on macOS. Existing threads switch providers at turn boundaries through app-server `thread/unsubscribe` and `thread/resume` requests.

Official OpenAI traffic never passes through the ProviderDeck helper. Before switching back to an official model, ProviderDeck locally checks rollout reasoning metadata and compacts non-replayable proxy reasoning with the original proxy provider. Conversation text is not returned to the renderer or logs, rewritten, or backed up; any required compaction is sent only to the proxy provider already used by that thread. Plugins, Skills, and MCP management remain with ChatGPT/Codex.
