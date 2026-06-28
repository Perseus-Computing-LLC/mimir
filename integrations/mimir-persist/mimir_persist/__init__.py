"""mimir-persist — a persistent FastMCP ``EventStore`` backed by SQLite.

MCP infrastructure: gives any FastMCP server SSE stream resumability across
restarts. See :class:`mimir_persist.store.MimirEventStore`.
"""

from mimir_persist.store import MimirEventStore

__all__ = ["MimirEventStore"]
__version__ = "0.1.0"
