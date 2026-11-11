# N-FDL DoS Vectors — Catalog

Явный перечень DoS-векторов, на которые ссылаются `05-verification.md`,
`11-error-diagnostics.md`, `12-testing.md` и `13-roadmap.md`. Каждый вектор
имеет стабильный `DiagId` / `AnomalyKind` и контракт «без паники».

| ID | Vector | Trigger | Spec anchor | Runtime outcome |
|---|---|---|---|---|
| DV-01 | NonProgressLoop | loop consumed 0 bytes | 05 §5.2, 03 §6 | `RuntimeSafetyAbort` + `NFDL0801` |
| DV-02 | LoopLimit | iterations > `max_loop_iterations` | 05 §5.2, 07 §10 | `RuntimeSafetyAbort` + `NFDL0802` |
| DV-03 | MaxDepthExceeded | bind depth > `max_layer_depth` | 07 §10, C7 | `Malformed::MaxDepth` + `NFDL0803` |
| DV-04 | ReassemblyLimit | flow buffer > limit | 08 §6 | `Anomaly(ReassemblyLimit)` + `NFDL0804` |
| DV-05 | SessionLimit | sessions > `max_sessions` | 09 §6.2 | LRU evict + `NFDL0805` |
| DV-06 | PluginTimeout | FFI > time budget | 10 §6.2 | `PluginError` + `NFDL0806` |
| DV-07 | CompressionBomb | plugin output ratio cap | 10 §6.2 | `PluginError` + `NFDL0807` |
| DV-08 | ParserStack | nesting > 64 | 01 §8 | `SyntaxError` + `NFDL0101` |
| DV-09 | SourceSize | source > 4 MiB | 01 §8 | `SyntaxError` + `NFDL0102` |
| DV-10 | OooBuffer | ooo_bytes > cap | 08 §6 | `Anomaly(TcpOverlap)` + `NFDL0808` |

**Acceptance (12-testing §4):** fuzz + property tests подтверждают, что каждый
вектор завершается recoverable-ошибкой, не panic/OOM.
