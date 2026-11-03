---
trigger: always_on
---

# nmem Protocol & Knowledge Persistence

1. **Authoritative Durable Memory**: `nmem` (Nowledge Mem CLI, configured at `~/.nowledge-mem/config.json`, pointing to `https://nmem.guohuasun.com`) is the durable memory system.
2. **ASCII Graphs**: Processes, architectures, root cause attributions, designs, state machines, and dependency relationships must use ASCII graphs in fenced `text` code blocks.
3. **Immutability & Semantic Evolution**: Memories in `nmem` are immutable. Never update or overwrite existing memories; record new iterations as new memories with explicit links.
4. **Structured Format**: Memory shapes must follow structured Markdown:
   - `# Title`
   - `## Conclusion`
   - `## Context`
   - `## Constraints`
   - `## Flow` (ASCII graph in fenced `text` block)
   - `## Evidence`
   - `## Next Actions`
