# ADGL Correctness Refinements — Audit Trail

**Пространства имён:** finding-IDs `A1..K3` ниже (находки critical-analysis) —
отдельное пространство от correction-IDs `C1..C12` (грамматика/ADR-list/examples).
Коллизия букв в группе C: находки `C1/C2/C3` (Ambiguity) ≠ correction
`C1/C2/C3` (execution/confidence/scope) — различайте по заголовку группы.

Метод: декомпозиция central claim («streaming DAG graph engine resolves Missing
Data Paradox with O(1) memory and lock-free parallel execution») на premises;
evidentiary inventory (prospect V1 §1–11 + AirPulse MCP `airpulse://*`);
logic/bias audit; alternative hypotheses; интегральная оценка.

**Вердict**: проспект архитектурно силён (watermark + bipartite + commutative
confidence — корректные идеи), но **неполон** для реализации: ~30 пробелов в
семантике, каталоге, edge-cases. После refinements — spec v1 self-consistent.

---

## A. Confidence scale & migration

| ID | Находка | Закрыто в |
|---|---|---|
| A1 | confidence 0..100 (new) vs 0..1 (legacy AirPulse verdict) — migration игнорирует конвертацию | [04-type-system.md](../spec/04-type-system.md) §2; [ADR-002](../adr/ADR-002-confidence-scale.md); legacy map `/100` в [11](../spec/11-error-diagnostics.md) |
| A2 | `weight` vs `confidence` semantics не зафиксированы (weight = delta? C start 0?) | [03-semantics.md](../spec/03-semantics.md) §3.3 (`C_new = clamp(0,100,C_old+W)`, C старт 0) |
| A3 | `EvidenceEdge::Contradicts` декларирован, но синтаксис `infer` только `+weight` | [02-grammar.ebnf](../spec/02-grammar.ebnf) §5 (`weight: ±`); [03](../spec/03-semantics.md) §3.3; [ADR-007](../adr/ADR-007-contradicts-and-suppression.md) |

## B. Scope vs target

| ID | Находка | Закрыто в |
|---|---|---|
| B1 | `target` и `scope` слиты | [09-scopes-sessions.md](../spec/09-scopes-sessions.md) §4; [04](../spec/04-type-system.md) §5; [ADR-003](../adr/ADR-003-scope-and-target.md) |
| B2 | Cross-scope: Cause в ClientMac не виден decision в Vlan (Example 5→2) | [09](../spec/09-scopes-sessions.md) §3 (roll-up ClientMac→Vlan, MAX); examples 03/04; [ADR-003](../adr/ADR-003-scope-and-target.md) |
| B3 | `Global` scope не определён (singleton? lock-free?) | [09](../spec/09-scopes-sessions.md) §6; [ADR-003](../adr/ADR-003-scope-and-target.md) |
| B4 | Multi-scope event routing (один event → Session/Vlan/Port) не определён | [09](../spec/09-scopes-sessions.md) §2 (fan-out по заявленным scope) |

## C. Ambiguity & exclusivity

| ID | Находка | Закрыто в |
|---|---|---|
| C1 | AmbiguityNode требует «mutually exclusive», но объявления нет | [02-grammar.ebnf](../spec/02-grammar.ebnf) §1 (`mutually_exclusive`); [10](../spec/10-catalog-abi.md) §7; [ADR-005](../adr/ADR-005-ambiguity-and-exclusivity.md) |
| C2 | AmbiguityNode → SARIF/JSON поток не определён | [03](../spec/03-semantics.md) §4 (`mark_ambiguous` → `Problem(AmbiguousDiagnosis)`); [10](../spec/10-catalog-abi.md) §4 |
| C3 | Ambiguity lifecycle (resolve/supersede) не определён | [03](../spec/03-semantics.md) §4; [ADR-005](../adr/ADR-005-ambiguity-and-exclusivity.md) |

## D. Temporal / watermark

| ID | Находка | Закрыто в |
|---|---|---|
| D1 | Watermark-политика не определена (replay vs live) | [08-stream-watermarking.md](../spec/08-stream-watermarking.md) §2; [ADR-004](../adr/ADR-004-watermark-policy.md) |
| D2 | Late events не обработаны | [08](../spec/08-stream-watermarking.md) §4 (offline accept+audit, live drop+side-output, allowed_lateness) |
| D3 | `MAX_LOOKBACK` vs windows — invariant не задан | [05-verification.md](../spec/05-verification.md) §3.1 (hard AOT); [08](../spec/08-stream-watermarking.md) §6 |
| D4 | Inclusive/exclusive границы `[a,b]` не зафиксированы | [02](../spec/02-grammar.ebnf) §4; [03](../spec/03-semantics.md) §3.2; [05](../spec/05-verification.md) §3.2 (inclusive both ends) |
| D5 | WaitQueue capacity — «O(1) memory» ложно для unbounded pending | [07-runtime.md](../spec/07-runtime.md) §3/§11 (`max_pending_per_scope` + spill); [ADR-004](../adr/ADR-004-watermark-policy.md) §WaitQueue bound; [ADR-011](../adr/ADR-011-dos-limits.md) |

## E. Catalog

| ID | Находка | Закрыто в |
|---|---|---|
| E1 | Event-схемы отсутствуют (verifier не может проверить metric-paths) | [10-catalog-abi.md](../spec/10-catalog-abi.md) §2 (12 events с полями) |
| E2 | Topology-функции без сигнатур | [10](../spec/10-catalog-abi.md) §6/§9 (6 функций, `T3`, cycle-bound) |
| E3 | Cause/Problem-схемы без полей | [10](../spec/10-catalog-abi.md) §3/§4; [04](../spec/04-type-system.md) §3 |
| E4 | Эксклюзивность-отношения отсутствуют | [10](../spec/10-catalog-abi.md) §7; [ADR-005](../adr/ADR-005-ambiguity-and-exclusivity.md) |
| E5 | `requires` (capabilities) без каталога/load-time check | [10](../spec/10-catalog-abi.md) §8; [05](../spec/05-verification.md) §6 |

## F. Determinism & emission

| ID | Находка | Закрыто в |
|---|---|---|
| F1 | Порядок firing/emission внутри partition не определён | [03-semantics.md](../spec/03-semantics.md) §6; [09](../spec/09-scopes-sessions.md) §5; [ADR-012](../adr/ADR-012-determinism.md) |
| F2 | SARIF stable `ruleId` из `Problem(X)` не определён; legacy совместимость | [ADR-008](../adr/ADR-008-sarif-mapping.md); [10](../spec/10-catalog-abi.md) §4 (`sarif_id`); [11](../spec/11-error-diagnostics.md) |
| F3 | Problem-дедуп не определён | [03](../spec/03-semantics.md) §3.4 (cooldown dedup) |
| F4 | Problem retraction при падении cause | [03](../spec/03-semantics.md) §3.4 (`superseded`); [ADR-007](../adr/ADR-007-contradicts-and-suppression.md) |
| F5 | Decision re-eval trigger не определён | [03](../spec/03-semantics.md) §3.5 (`ConfidenceMutation` → re-check) |

## G. Bipartite & actions

| ID | Находка | Закрыто в |
|---|---|---|
| G1 | «evidence only mutate graph state» ложно (request_observation) | [03-semantics.md](../spec/03-semantics.md) §2; [ADR-006](../adr/ADR-006-bipartite-isolation.md); [05](../spec/05-verification.md) §8 |
| G2 | Action-семантика per mode (offline/live) не определена | [03](../spec/03-semantics.md) §3.6; [10](../spec/10-catalog-abi.md) §10 (`ActionSink`, `RunMode`) |
| G3 | Decision anchor на Problem (Example 8) расширяет bipartite | [02-grammar.ebnf](../spec/02-grammar.ebnf) §3 (`ProblemAnchor`); [03](../spec/03-semantics.md) §3.5; [ADR-006](../adr/ADR-006-bipartite-isolation.md) |

## H. Privacy

| ID | Находка | Закрыто в |
|---|---|---|
| H1 | strict-mode PII/redaction не формализован | [ADR-009](../adr/ADR-009-privacy-strict.md); [10](../spec/10-catalog-abi.md) §2/§11; [11](../spec/11-error-diagnostics.md) §5 |

## I. Topology Unknown

| ID | Находка | Закрыто в |
|---|---|---|
| I1 | `Unknown` ≠ `absent` (prospect противоречит себе в §11) | [ADR-010](../adr/ADR-010-topology-unknown.md); [03](../spec/03-semantics.md) §3.2/§3.7 (three-valued Kleene); [10](../spec/10-catalog-abi.md) §6 |

## J. Versioning

| ID | Находка | Закрыто в |
|---|---|---|
| J1 | `version = "1.0"` — semver/compat политика не задана | [01-lexical.md](../spec/01-lexical.md) §3.1; [06-ir-bytecode.md](../spec/06-ir-bytecode.md) §2 (`version: u32` semver-packed); [ADR-list](../adr/ADR-list-critical-decisions.md) |

## K. DoS / limits

| ID | Находка | Закрыто в |
|---|---|---|
| K1 | Lexer/parser лимиты (mirror NFDL §8) | [01-lexical.md](../spec/01-lexical.md) §8; [05](../spec/05-verification.md) §9 |
| K2 | Runtime лимиты (pending/causes/ringbuffer/hops/firings) | [07-runtime.md](../spec/07-runtime.md) §3/§11; [ADR-011](../adr/ADR-011-dos-limits.md) |
| K3 | Topology cycle bound | [07](../spec/07-runtime.md) §6 (`max_topology_hops` + visited); [10](../spec/10-catalog-abi.md) §6; [ADR-010](../adr/ADR-010-topology-unknown.md) |

---

## Альтернативные гипотезы (critical-analysis)

1. **«O(1) memory» правда** — опровергнуто: WaitQueue O(active windows); corrected
   to «amortized O(active windows), bounded `max_pending_per_scope`» (D5).
2. **«lock-free parallel execution» без оговорок** — уточнено: cross-partition
   lock-free, intra-partition serial (C12); Global — точка сериализации (B3).
3. **«routes Unknown as absent»** — внутреннее противоречие проспекта (§11 vs
   «not assuming false»); resolved: Unknown ≠ absent, three-valued (I1).

## Интегральная оценка

- **Evidentiary strength**: strong архитектурные идеи (watermark, bipartite,
  commutative confidence) backed AirPulse MCP rules/concepts; weak — каталог и
  edge-cases (группы E, D, H, I).
- **Logical integrity**: одно внутреннее противоречие (I1) — устранено.
- **Bias assessment**: prospect оптимистичен re memory/ordering («O(1)»,
  «autonomous ambiguity») — корректировки вводят явные лимиты и объявления.
- **Verdict**: Moderate → (после refinements) **Strong / spec-ready**.

## Checkpoint (critical-analysis protocol)

- Search contradictory evidence? — AirPulse `airpulse://rules` (flat TOML)
  подтверждает необходимость migration; Flink/SARIF/winnow — внешние факты
  согласованы (Exa-verified, [13-roadmap](../spec/13-roadmap.md) §Внешние ссылки).
- Deeper methodology dive? — temporal-bounds verifier (05 §3) — formal
  interval-style proof sketched; Z3-опц. в v1.5 (как N-FDL ADR-003).
- Author credentials/funding? — N/A (internal architecture proposal).

---

## Pass 2 (2026-06-28): grammar / semantics correctness audit

Второй проход `critical-analysis` готового каталога `docs/idea/` — проверка
внутренней согласованности spec ↔ grammar ↔ examples (не conventions/deslop,
которые закрыты Pass 1). Найдены и закрыты 11 дефектов:

| # | Дефект | Закрыто в |
|---|---|---|
| P2-1 | `DecisionAnchor` требовал литерал `";"`, но ни один example его не ставил (asymmetry vs `AnchorBlock`) | [02-grammar.ebnf](../spec/02-grammar.ebnf) §3 (убран `";"`) |
| P2-2 | `IfElseBlock` cond = только `present`/`absent`, но Example 7/06 смешивает `present(oper) and oper.state == "DOWN"` (metric-pred) | [02](../spec/02-grammar.ebnf) §6/§8 (`present`/`absent` → `Primary`, cond = `Expr`); [03](../spec/03-semantics.md) §3.7 (T3-Expr + short-circuit) |
| P2-3 | **Направление `⊑` инвертировано** системно: (T-Infer)/(T-Emit)/05 §4/09 §4 требовали `target ⊑ rule.scope` (target — child), но cross-scope roll-up (Example 5) — наоборот (rule ClientMac → target Vlan = ancestor). Отвергало канонический example. | [04](../spec/04-type-system.md) §7/§9; [05](../spec/05-verification.md) §1/§4/§11; [09](../spec/09-scopes-sessions.md) §4/§9; [11](../spec/11-error-diagnostics.md) example — везде `rule.scope ⊑ target-scope` (target — ancestor-or-equal) |
| P2-4 | Типизация `c.target`, `c.confidence`, `p.target`, `p.severity` не задана (только `rtx.f` через (T-Anchor)); Example 2 использует `c.target` | [04](../spec/04-type-system.md) §7 — (T-Anchor) → обобщён (T-NodeField) для Event/Cause/ProblemRef |
| P2-5 | Correlate binding описан только для `event` (`Ring.scan`); Example 8 correlate на `Problem` — не покрыт | [03](../spec/03-semantics.md) §3.2 — `candidates` по `CorrelateSource` (event→Ring, Problem/Cause→SubGraph); [06](../spec/06-ir-bytecode.md) §2.1 `CorrelateSpec.source` + `LOAD_PROBLEM_FIELD` |
| P2-6 | Problem-anchor decision trigger не формализован (03 §3.5 упоминал, но без правила) | [03](../spec/03-semantics.md) §3.5 — `on ProblemEmission(P,T,SG) → DecisionRule с ProblemAnchor(P)` |
| P2-7 | `action request_observation { target: rtx.path }` — `rtx.path : List<ScopeId>`, но target-тип action не определён (ожидался `ScopeId`) | [10](../spec/10-catalog-abi.md) §5 (`target: ScopeId \| List<ScopeId>` для path); [04](../spec/04-type-system.md) §7 (T-Action, per-kind target types) |
| P2-8 | `action request_observation(icmp.visibility)` — аргумент dotted (`Ident . Ident`), но грамматика `[ "(" Ident ")" ]` (dot-free `ident`, 01 §3) — syntax error | [02](../spec/02-grammar.ebnf) §9 (`KindIdent ::= Ident { "." Ident }`); `ActionStmt` arg → `KindIdent`; `EventType ::= KindIdent` |
| P2-9 | Example 8 ссылается на `Problem(DeviceUnreachable)` без emitter'а в ruleset — неявная cross-rule зависимость | [examples/07](../examples/07-suppress-downstream.adgl) — note про emitter в том же ruleset (v1) / cross-ruleset v1.5+ |
| P2-10 | `8021x.eapol_start` — первый сегмент начинается с цифры; `ident` (01 §3) требует letter/`_` start, а maximal-munch ломает `8021x` → `IntLit(8021)`+`Ident(x)`. Переименовано в `dot1x.*` (legacy-consistent `l3_dot1x_wired`) | [10](../spec/10-catalog-abi.md) §2; [examples/04](../examples/04-dhcp-missing-auth.adgl); [plans/migration](migration-from-flat-toml.md) |
| P2-11 | Rule-DAG §2.1 третье ребро (`evidence R1 infer K → evidence R3 infer K` через exclusivity) создавало self-loop (Example 2: одно правило, 3 mutually-exclusive causes) и 2-цикл (Example 10: два правила, exclusivity-пара) ⇒ Tarjan SCC отверг бы валидные ambiguity-seed примеры. Ambiguity synthesis — read-only, не re-fire'ит правила ⇒ не execution-dependency. | [05](../spec/05-verification.md) §2.1 (убрано третье ребро; note про read-only synthesis) |

**Метод**: argument-map (central claim = «каждый example парсится per грамматикой,
каждый ref разрешим через каталог, spec внутренне согласован»); evidentiary
inventory = grammar(02) × examples(10) × catalog(10) × semantics/verifier/types;
logic audit = направление `⊑`, three-valued short-circuit, correlate-source
coverage; alternative = ручной перебор каждого example против fixed grammar.

**Verdict**: после Pass 2 — все 10 examples парсятся per [02](../spec/02-grammar.ebnf),
все event/cause/problem/action/topology refs разрешимы per [10](../spec/10-catalog-abi.md),
`⊑`-направление согласовано через 04/05/09/11, correlate/Problem-anchor/KindIdent
покрыты в грамматике, семантике и IR, rule-DAG (05 §2) не отвергает ambiguity-seed
примеры (02/10). Spec v1 — self-consistent и example-valid.

---

## Pass 3 (2026-06-28): runtime / watermark / ABI / catalog-count audit

Третий проход `critical-analysis` — намеренно против confirmation-bias Pass 2
(автор правок Pass 2 ≈ biased «свои правки верны»). Пере-проверены файлы, **не**
затронутые Pass 2 (runtime [07](../spec/07-runtime.md), testing [12](../spec/12-testing.md),
roadmap [13](../spec/13-roadmap.md), ADR-001..012, plans/{test-plan, implementation-phases}),
+ cross-cutting: trait-ABI (07↔10↔04↔ADR-010) и watermark-граница (03↔07↔08).
Найдены и закрыты 8 дефектов:

| # | Дефект | Закрыто в |
|---|---|---|
| P3-1 | `TopologyProvider::upstream_of(up, down, max_hops: u8)` ([07](../spec/07-runtime.md) §6) vs `(up, down)` без param ([10](../spec/10-catalog-abi.md) §9, [ADR-010](../adr/ADR-010-topology-unknown.md), [04](../spec/04-type-system.md) §6.3 — `max_hops` baked in impl). Signature drift. | [07](../spec/07-runtime.md) §6 — убран `max_hops` param |
| P3-2 | `ActionSink::emit(intent, mode)` ([07](../spec/07-runtime.md) §7) vs `emit(intent, mode, wm: i64)` ([10](../spec/10-catalog-abi.md) §10) — отсутствует watermark-параметр для упорядочивания эффектов (C12) | [07](../spec/07-runtime.md) §7 — добавлен `wm: i64` |
| P3-3 | `RunMode::Live { ebpf }` ([07](../spec/07-runtime.md) §7) без `topo`, но §7 prose `request_topology → topology.poll(scope)` и [10](../spec/10-catalog-abi.md) §10 `Live { ebpf, topo }` — внутренняя + cross-spec нестыковка | [07](../spec/07-runtime.md) §7 — `Live { ebpf, topo: &mut TopologyController }` |
| P3-4 | **Граница suspend/resume**: [08](../spec/08-stream-watermarking.md) §3.2 resume строго `wm > upper_bound`, но [03](../spec/03-semantics.md) §3.1 / [07](../spec/07-runtime.md) §5 ingest / [08](../spec/08-stream-watermarking.md) §3.1 suspend = `upper > wm` (non-strict ⇒ execute при `wm == upper`). На границе `wm == upper` — absent-risk (out-of-order event ровно в `upper`); Flink watermark = strict. | [03](../spec/03-semantics.md) §3.1; [07](../spec/07-runtime.md) §5 ingest; [08](../spec/08-stream-watermarking.md) §3.1/§5 — suspend при `upper >= wm`, execute/resume при `wm > upper` |
| P3-5 | `advance_watermark`: `let wm = fetch_max(t, ..)` использует возвращаемое (старое) значение в resume-loop ⇒ stale watermark | [07](../spec/07-runtime.md) §5 — `fetch_max(t, ..).max(t)` (new wm) |
| P3-6 | **Счётчики каталога**: [10](../spec/10-catalog-abi.md) §2 «Events (11)» (фактически 12), §3 «Causes (7)» (8), §4 «Problems (5+Amb)» (6+Amb) — добавлены `dot1x`/`port.*` events, `DeviceUnreachable`, `PhysicalLinkAbsent`, `UpstreamOutage`, но заголовки/roadmap/тесты не обновлены | [10](../spec/10-catalog-abi.md) §2/§3/§4; [13](../spec/13-roadmap.md) M4; [12](../spec/12-testing.md) §5; [implementation-phases](implementation-phases.md); E1 row выше — везде 12/8/6+Amb |
| P3-7 | **End-of-stream flush (offline) не специфицирован**: при finite PCAP pending с `upper > last_event_time` никогда не получат `wm > upper` ⇒ absent-branch не разрешается ⇒ golden PMTUD-absent ([13](../spec/13-roadmap.md) M0) не триггерил бы | [08](../spec/08-stream-watermarking.md) §3.4 (flush `wm := +∞`, resume-loop, gc) |
| P3-8 | Trait bounds: `trait TopologyProvider {` / `trait ActionSink {` ([07](../spec/07-runtime.md) §6/§7) без `Send`/`Sync`, но [10](../spec/10-catalog-abi.md) §9/§10 требуют `: Send`(+`Sync`) для cross-partition lock-free (07 §10) | [07](../spec/07-runtime.md) §6 (`: Send + Sync`), §7 (`: Send`) — merged с P3-1/P3-2 |

**Метод**: argument-map (central claim после Pass 2 = «spec v1 self-consistent и
example-valid»); evidentiary inventory = untouched-by-Pass-2 files (07/12/13,
ADR-001..012, plans) + cross-cutting (trait-ABI 07↔10, watermark-boundary
03↔07↔08, catalog-counts 10↔13↔12↔phases); logic audit = suspend/resume boundary
математика, `fetch_max` return-value, offline-терминация; bias audit = активный
поиск регрессий от собственных Pass 2 правок; alternative = «Pass 2 внёс новую
нестыковку» (не подтвердилось для Pass 2; P3-4 — residual с Pass 1).

**Verdict**: после Pass 3 — trait-ABI (07↔10↔04↔ADR-010) согласован,
watermark-boundary strict-согласован (03↔07↔08) + end-of-stream flush закрыл
offline-терминацию, счётчики каталога (12/8/6+Amb) синхронизированы через
10/13/12/phases. Spec v1 — self-consistent, example-valid, runtime-terminate-correct.

---

## Pass 4 (2026-06-28): empirical example-parse audit + P3-4 regression revert

Четвёртый проход `critical-analysis` — намеренно **эмпирический**: ручной
parse-trace каждого из 10 `.adgl` examples против финальной
[02](../spec/02-grammar.ebnf) + лексики [01](../spec/01-lexical.md) (активная
проверка альтернативной гипотезы «examples не парсятся / содержат type-ошибки»).
Это вскрыло 3 дефекта, включая **регрессию, введённую Pass 3** (P3-4 был false
positive):

| # | Дефект | Закрыто в |
|---|---|---|
| P4-1 | **Grammar ↔ examples: literal `";"`**. Grammar требовал `";"` после `version`/`requires`/`mutually_exclusive`/`scope`/`topo`/`time` (`Version ::= "version" "=" StringLit ";"`, `"scope" ":" ScopeType ";"`, `"topo" ":" TopoPredicate ";"` …), но **все 10 examples** опускают `;` (консистентный keyword-delimited стиль). Ни один example не парсился. | [02](../spec/02-grammar.ebnf) §1/§2/§3/§4 — убраны literal `";"`; `RulesetHeader` реструктурирован в `Version { Decl }`, `Decl ::= requires \| mutually_exclusive`; [09](../spec/09-scopes-sessions.md) §3.3 prose `scope: Vlan;` → `scope: Vlan` |
| P4-2 | **P3-4 был false positive (self-introduced regression)**. Pass 3 сделал suspend строгим (`upper >= wm`), чтобы «согласовать» с strict resume (`wm > upper`). Но original `upper > wm` (non-strict suspend) **уже** был согласован со strict resume: suspended-pending всегда имеет `upper > wm_at_suspend`, поэтому resume `wm > upper` корректно его закрывает — никакой pending не suspended при `upper == wm`. Строгий suspend сломал backward-only rules (Example 8: `upper == wm == rtx.time` ⇒ suspend вместо documented «executes immediately on anchor match»). | [03](../spec/03-semantics.md) §3.1; [07](../spec/07-runtime.md) §5 ingest; [08](../spec/08-stream-watermarking.md) §3.1 — revert `upper >= wm` → `upper > wm` (non-strict suspend; backward-only `upper == wm` ⇒ immediate, Example 8); **keep** strict resume (08 §3.2 `wm > upper`), 08 §5 «выше rtx.time+1s», end-of-stream flush (08 §3.4), fetch_max `.max(t)` (07 §5) — всё из P3-4/P3-5/P3-7 остаётся в силе |
| P4-3 | **`Problem.time` / `Cause.time` не определены, но используются**. Example 7 `correlate upstream: Problem(...) { time: upstream.time in [downstream.time - 30s, …] }` ссылается на `upstream.time`/`downstream.time` (Problem bindings), и 03 §3.2 фильтрует `Problem(K)`/`Cause(K)` по `time.window` — но `ProblemNode`/`CauseNode` (04 §3, 10 §3/§4) не имели поля `time` ⇒ type error. | [04](../spec/04-type-system.md) §3 (CauseNode/ProblemNode + `time: Int`), §6.2 (schemas), §7 (T-NodeField field-sets); [10](../spec/10-catalog-abi.md) §3/§4 prose; [03](../spec/03-semantics.md) §3.3 (Cause.time = first-infer Evt.time, stable), §3.4 (Problem.time = emission WM); [06](../spec/06-ir-bytecode.md) §4 (LOAD_*_FIELD comments + `.time`) |

**Метод**: argument-map (central claim после Pass 3 = «spec v1 self-consistent,
example-valid, runtime-terminate-correct»); evidentiary inventory = empirical
parse-trace всех 10 examples × финальная grammar (02) + lexica (01) + catalog
(10) + types (04); logic audit = semicolon-terminators, suspend/resume boundary
математика (re-derived: non-strict suspend + strict resume ⇒ consistent —
contrapositive of P3-4), node-field type-completeness; **bias audit = активный
поиск собственных регрессий Pass 1–3** — подтвердилось: P3-4 был false positive
(confirmation bias на «strict безопаснее» проигнорировала backward-only semantics
и documented Example 8 contract); alternative = «examples не парсятся»
(подтвердилось для `";"` — P4-1) и «examples type-error» (подтвердилось для
Problem.time — P4-3).

**Verdict**: после Pass 4 — все 10 examples парсятся per [02](../spec/02-grammar.ebnf)
(0 literal `";"` в productions), type-check per [04](../spec/04-type-system.md)/[10](../spec/10-catalog-abi.md)
(Cause/Problem `.time` определены), backward-only immediate-execution (Example 8)
восстановлена (P3-4 regression reverted), strict resume + end-of-stream flush
сохранены. Spec v1 — self-consistent, **example-valid (empirically verified)**,
runtime-terminate-correct. **Meta-урок**: Pass 3 confirmation-bias произвела false
positive (P3-4); эмпирический parse-trace examples (Pass 4) — обязательная
проверка, counter-balancing авторские правки.

---

## Pass 5 (2026-06-28): verifier-spec + unaudited-ADR deep read

Пятый проход `critical-analysis` — ортогональное измерение к Pass 2–4: глубокий
re-read файлов, **только cross-referenced** ранее, не deeply-audited — verifier
[05](../spec/05-verification.md) (модифицирован Pass 2/4, но не re-read) и ADR-001/002/005/008/009/012
+ [ADR-list](../adr/ADR-list-critical-decisions.md) (исполнение, confidence,
ambiguity, SARIF, privacy, determinism). Цель: проверить, что поздние правки
(P2-3 `⊑`-flip, P2-8 KindIdent, P2-11 rule-DAG, P4-3 `time`-fields) не оставили
inconsistencies в верификаторе и ADR. Найдены и закрыты 4 дефекта:

| # | Дефект | Закрыто в |
|---|---|---|
| P5-1 | **Action-arg verification underspecified**. [05](../spec/05-verification.md) §1 pipeline гласил лишь «action + arg-kind existence», но `suppress_symptom(downstream)` (Example 7) принимает **ProblemRef binding**, а `request_observation(icmp.visibility)` (Examples 1/4/9) — **catalog observation kind**. Грамматика (02 §9) унифицирует оба как `KindIdent`; различение — задача верификатора по `ActionKind`. Без явного правила имплементёр, читающий «arg-kind existence», отверг бы `suppress_symptom(downstream)` (downstream — не catalog kind). Дополнительно 11 не имела diagnostic-IDs для action/arg catalog-resolution ошибок. | [05](../spec/05-verification.md) §1.1 (новая подтаблица per-ActionKind: catalog-kind vs ProblemRef binding vs no-arg; lowering к `EmitAction`/`SupersedeProblem`); [11](../spec/11-error-diagnostics.md) §2.1 — `ADGL0206 UnknownActionKind`, `ADGL0207 UnknownObservationKind`, `ADGL0208 UnknownCheckKind`, `ADGL0209 ActionArgNotProblemRef` |
| P5-2 | ADR-list typo: «не ломаетlock-free-заявление» (слитно, пропущен пробел) | [ADR-list](../adr/ADR-list-critical-decisions.md) ADR-003 — «не ломает lock-free-заявление» |
| P5-3 | ADR-009 утверждала «`RunMode` config: `strict: bool` (07 §7)», но [07](../spec/07-runtime.md) §7 `RunMode = Offline { audit } \| Live { ebpf, topo }` **не несёт** `strict`-флага — privacy-strict ортогонален sink-dispatch и живёт как runtime-config-param в [10](../spec/10-catalog-abi.md) §11 `redact_evidence(..., strict: bool)`. Misleading reference (имплементёр не нашёл бы `strict` в 07 §7). | [ADR-009](../adr/ADR-009-privacy-strict.md) — «runtime config: `strict: bool` (10 §11 `redact_evidence`; orthogonal to `RunMode`, applies to both Offline/Live)» |
| P5-4 | **10 §5 action-signature lexical category уже грамматики**. `request_observation(kind: Ident)` / `run_check(kind: Ident)`, но observation kinds **dotted** (`icmp.visibility`, `aaa.telemetry`, `wifi.rf_metrics` = `KindIdent`, не `Ident`); grammar (02 §9) использует `KindIdent` для action-args. `Ident` (single, dot-free) сделал бы Examples 1/4/9 self-inconsistent (dotted arg ≠ `Ident`). Зеркало P2-8 (grammar был уже `KindIdent`, каталог отстал). | [10](../spec/10-catalog-abi.md) §5 — `kind: Ident` → `kind: KindIdent` для `request_observation`/`run_check` (согласовано с 02 §9, 04 §6.4 указывает на 10 §5) |

**Метод**: argument-map (central claim после Pass 4 = «spec v1 self-consistent,
example-valid, runtime-terminate-correct»); evidentiary inventory = verifier (05)
× catalog (10 §5) × grammar (02 §9) × IR (06 §2.3) × types (04 §6.4/§7) × diag
(11) × ADRs (001/002/005/008/009/012/list); logic audit = per-ActionKind
arg-typing (catalog-kind vs Ref-binding), lexical-category congruence
(Ident/KindIdent) grammar↔catalog, cross-spec reference accuracy (strict↔RunMode),
ADR↔spec consistency (08 partialFingerprints, 12 fetch_max/ordering — both
confirmed consistent, no defect); bias audit = проверка, что поздние правки не
внесли новых inconsistencies в ранее-audited-только-cross-ref файлы; alternative
= «verifier отвергает валидный example из-за arg-typing» (подтвердилось для
suppress_symptom — P5-1) и «catalog stricter чем grammar» (подтвердилось — P5-4).

**Вердict**: после Pass 5 — verifier (05) явно типизирует action-args per
`ActionKind` (§1.1) + 4 новых diagnostic-ID (ADGL0206-0209); catalog (10 §5)
lexical-category согласован с grammar (KindIdent); ADR-009 reference исправлен;
ADR-list typo исправлен. ADR-001/002/005/008/012 — cross-consistent со spec без
дефектов (execution-model, confidence-scale, ambiguity-synthesis, SARIF-mapping,
determinism — все согласованы с 03/06/07/10). Spec v1 — self-consistent,
example-valid, runtime-terminate-correct, **verifier-complete**. **Meta-урок**:
ортогональный deep-re-read ранее-только-cross-referenced файлов (verifier + ADRs)
вскрыл 2 underspecification-дефекта (P5-1/P5-4), не обнаружимых parse-trace'ем
examples (examples валидны по построению — verifier их бы принял, но spec не
опубликовал правила); каждый класс дефекта требует своего метода аудита.

---

## Pass 6 (2026-06-28): plans + testing/roadmap audit, fact-checked vs airpulse://rules

Шестой проход `critical-analysis` — ортогональное измерение: глубокий re-read
plans (migration/test-plan/implementation-phases) + [12](../spec/12-testing.md)/[13](../spec/13-roadmap.md),
ранее lightly-touched (Pass 3 — только счётчики), не deeply-audited на drift vs
финальных specs. **Ключевое**: principle #4 (Fact-Check First) — legacy-ID claims
проверены против authoritative source `airpulse://rules` (AirPulse MCP), а не
приняты на веру. Это вскрыло кластер дефектов (P6-2), не обнаружимый 5 passes
документ-кросс-референсинга (все доверяли заявленным legacy IDs). Найдены и
закрыты 2 дефекта:

| # | Дефект | Закрыто в |
|---|---|---|
| P6-1 | migration §6 acceptance «Adapter покрывает все **11** catalog events» — count drift, пропущен P3-6 (10 §2/13/12/phases обновлены до 12, migration §6 — нет) | [plans/migration](migration-from-flat-toml.md) §6 — «12 catalog events» |
| P6-2 | **Legacy-ID mapping error cluster (verified vs `airpulse://rules`)**. (a) `xl_icmp_tcp_mss` — **не существует** как legacy rule id (реальные PMTUD-MSS: `l3_icmp_tcp_mss_loss`/`_rst`/`l3_icmp_tcp_blackhole_loss`). (b) catalog `XlIcmpTcpMss.sarif_id = "l3_xl_icmp_tcp_mss"` не совпадает **ни с одним** legacy id, тогда как STP (`l3_stp_spanning_tree`) и dot1x (`l3_dot1x_wired`) корректно = legacy `recommendation_id` ⇒ PMTUD ломает pattern. (c) «1:1 `fired_rule_ids` parity» **невозможно**: legacy имеет 4 PMTUD rule ids → 1 ADGL Problem (many-to-one). | [10](../spec/10-catalog-abi.md) §4 — `XlIcmpTcpMss.sarif_id` `"l3_xl_icmp_tcp_mss"` → `"l3_pmtud_blackhole"` (real legacy `recommendation_id` of `l3_icmp_tcp_blackhole_loss`; consistent с STP/dot1x pattern; matches Example 01 blackhole semantic); §4 prose + §12 contract — reframed «1:1» → verdict-level parity; [04](../spec/04-type-system.md) §6.2 — sarif_id literal; [examples/01](../examples/01-pmtud-blackhole.adgl) — emit sarif_id; [examples/README](../examples/README.md) — legacy-ID column + §изменения; [ADR-008](../adr/ADR-008-sarif-mapping.md) §ruleId example + §Legacy (→ «Legacy mapping»: sarif_id=legacy `recommendation_id`, many-to-one, verdict-level parity via adapter `legacy_rule_id→sarif_id`) + rejected-alt; [ADR-list](../adr/ADR-list-critical-decisions.md) ADR-008 summary; [plans/migration](migration-from-flat-toml.md) §2 — «Stable ID parity» reframed (raw `fired_rule_id` 1:1 impossible → verdict-level via adapter mapping; non-existent `xl_icmp_tcp_mss` removed; real legacy PMTUD rule ids cited) |

**Метод**: argument-map (central claim после Pass 5 = «spec v1 self-consistent,
example-valid, runtime-terminate-correct, verifier-complete»); evidentiary
inventory = plans (migration/test-plan/implementation-phases) × 12-testing ×
13-roadmap × catalog (10 §4 sarif_ids) × ADR-008/ADR-list × **`airpulse://rules`
(authoritative legacy flat-TOML)**; logic audit = catalog-count consistency,
legacy-ID existence + sarif_id=recommendation_id pattern, many-to-one parity
feasibility; **bias audit = активный fact-check против MCP ground-truth**
(principle #4 — не доверять заявленным IDs; `xl_icmp_tcp_mss` опровергнуто, 4
реальных PMTUD IDs подтверждены); alternative = «legacy IDs в docs неверны»
(подтвердилось — P6-2) и «count drift в plans» (подтвердилось — P6-1).

**User decision**: P6-2 потребовал выбора stable PMTUD `sarif_id` (genuine design
decision: `l3_pmtud_blackhole` [Example 01 blackhole semantic] vs
`l3_pmtud_investigate` [`xl_icmp_tcp_mss` condition's rec] vs keep
`l3_xl_icmp_tcp_mss` [new symbolic]). AskQuestion skipped → applied recommended
option `l3_pmtud_blackhole` (matches Example 01 semantic + real legacy
`recommendation_id` + STP/dot1x pattern consistency). Все 3 option'а включали
fix non-existent id + reframe «1:1 fired_rule_ids».

**Вердict**: после Pass 6 — catalog sarif_ids для legacy-covered диагнозов
= legacy `recommendation_id` (PMTUD `l3_pmtud_blackhole`, STP `l3_stp_spanning_tree`,
dot1x `l3_dot1x_wired`; verified vs `airpulse://rules`); `ap_*` — новые IDs
(legacy wired-TOML без wifi/AP/L2/global-эквивалентов); parallel-run parity
честно смоделирована как verdict-level (many-to-one legacy rule ids → sarif_id
via adapter mapping, не raw `fired_rule_id` equality); migration §6 count
синхронизирован (12). 12-testing/13-roadmap — consistent без дефектов. Spec v1 —
self-consistent, example-valid, runtime-terminate-correct, verifier-complete,
**migration-parity-accurate (fact-checked vs AirPulse MCP)**. **Meta-урок**:
5 passes документ-кросс-референсинга не вскрыли P6-2 — все доверяли заявленным
legacy IDs; один fact-check против authoritative source (`airpulse://rules`)
решил вопрос за 30с. Principle #4 (Fact-Check First) — обязательна для claims
о внешних системах; кросс-референсинг внутри docs ≠ верификация против ground
truth.

---

## Pass 7 (2026-06-30): Regression re-audit + plans drift check

Седьмой проход — delta после Pass 6, pre-implementation refresh. Inventory:
все `docs/idea/spec/` + `adr/` + `plans/` + 10 examples; cross-check N-FDL audit
artifacts не сломали shared conventions.

| # | Проверка | Result |
|---|---|---|
| P7-1 | Catalog counts (12 events, 8 causes, 6+Amb problems) ↔ migration §6 | consistent |
| P7-2 | `mutually_exclusive` / KindIdent / semicolon style in 10 examples | no regression |
| P7-3 | ADR-001..012 Status + cross-refs to spec § | consistent |
| P7-4 | Legacy sarif_id pattern (PMTUD `l3_pmtud_blackhole`, STP, dot1x) | unchanged since P6-2 |
| P7-5 | Shared doc conventions with N-FDL (01–13 numbering, C-ID scheme) | aligned |

**External fact-check:** `airpulse://rules` MCP unavailable in cloud agent env;
legacy-ID claims from Pass 6 retained as verified baseline — re-run MCP locally
before implementation M0.

**Verdict:** ADGL spec v1 remains **self-consistent, example-valid,
migration-parity-accurate** (no new defects). N-FDL Pass 1–6 artifacts added under
[`docs/plans/`](../plans/) without conflicting ADGL correction-ID namespace.


