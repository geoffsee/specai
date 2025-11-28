# spec-ai-knowledge-graph

Knowledge graph storage and synchronization for the spec-ai framework.

## Overview

This crate provides isolated knowledge graph functionality, including:

- **Graph Store**: DuckDB-based storage for graph nodes and edges
- **Vector Clocks**: Logical clocks for distributed synchronization
- **Graph Types**: Core types for nodes, edges, queries, and traversals

## Key Components

### Graph Store (`graph_store`)
- `KnowledgeGraphStore` - Main storage interface for graph operations
- `SyncedNodeRecord` / `SyncedEdgeRecord` - Records with sync metadata
- `ChangelogEntry` - Change tracking for synchronization

### Types (`types`)
- `GraphNode` / `GraphEdge` - Core graph primitives
- `NodeType` / `EdgeType` - Type classifications
- `GraphQuery` / `GraphQueryResult` - Query structures
- `GraphPath` - Path traversal results
- `TraversalDirection` - Edge traversal direction

### Vector Clock (`vector_clock`)
- `VectorClock` - Logical clock for causality tracking
- `ClockOrder` - Ordering comparison results

## Usage

This is an internal crate used by:
- `spec-ai-core` - For graph operations in the agent runtime
- `spec-ai-config` - For persistence layer integration

For end-user documentation, see the main [spec-ai README](../../README.md).
