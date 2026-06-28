# ADGL Error Diagnostics v1

Определяет таксономию ошибок, AOT vs runtime фазный контракт, ariadne-rendering,
event bus, privacy-telemetry и DoS-mapping. Crate-владелец `airpulse_dsl::diag`.
Зеркалирует N-FDL [../../spec/11-error-diagnostics.md](../../spec/11-error-diagnostics.md);
`Diagnostic { id, severity, span, message, labels, help }`, stable `ADGL####` IDs.

## 1. Таксономия

```
Error =
  | SyntaxError     (lexer/parser, AOT)         ADGL01xx
  | TypeError       (04, AOT)                    ADGL02xx
  | VerificationError (05, AOT)                  ADGL04xx / ADGL05xx
  | CorrelateError  (runtime, degrade)           ADGL30xx
  | LimitExceeded   (runtime, DoS)               ADGL31xx
  | ActionSinkError (runtime, external)          ADGL32xx
```

### 1.1 Фазный контракт

| Ошибка | Фаза | Эффект | Паника ядра? |
|---|---|---|---|
| SyntaxError | AOT | reject spec | нет |
| TypeError | AOT | reject spec | нет |
| VerificationError | AOT | reject spec | нет |
| CorrelateError | runtime | локальный degrade (skip rule / audit) | нет |
| LimitExceeded | runtime | spill/drop + diagnostic | нет |
| ActionSinkError | runtime | audit + continue | нет |

AOT-ошибки — батч (как rustc), сортировка по span. Runtime — event bus, не паника.

## 2. AOT-диагностика (ariadne)

`Diagnostic { id: DiagId, severity: Severity, span: Span, message: String,
labels: Vec<(Span, String)>, help: Option<String> }`. `DiagId` = `"ADGL0412"`
→ `error[ADGL0412]:`.

Пример (scope/target mismatch, C3):

```
error[ADGL0210]: target scope not compatible with rule scope
  --> rulesets/airpulse.adgl:42:18
   |
42 |   infer Cause(RfInterference) { target: rtx.target, weight: +40 }
   |                  ^^^^^^^^^^^^^   ^^^^^^^^^^
  = rule.scope AccessPoint ⊑ target.scope Session is false (sibling scopes, 04 §5);
    RfInterference requires AccessPoint target, but rtx.target is Session
  help: anchor a wifi.* event whose target is AccessPoint, or change scope
```

ariadne source-span rendering (как N-FDL, rustc-style). Несколько ошибок — батч.

### 2.1 Selected AOT diagnostic IDs

```
ADGL0101  IdentifierExceeds255
ADGL0102  SourceExceeds4MiB
ADGL0103  NestingExceeds64
ADGL0104  ReservedDoubleUnderscore
ADGL0105  TooManyRequires
ADGL0110  MalformedDurationUnit
ADGL0201  UnknownEventType
ADGL0202  UnknownCauseKind
ADGL0203  UnknownProblemKind
ADGL0204  TooManyCorrelates
ADGL0205  TooManyIntents
ADGL0206  UnknownActionKind          (05 §1.1)
ADGL0207  UnknownObservationKind     (05 §1.1)
ADGL0208  UnknownCheckKind           (05 §1.1)
ADGL0209  ActionArgNotProblemRef     (05 §1.1)
ADGL0210  ScopeTargetMismatch        (C3)
ADGL0211  CauseScopeInvalid          (05 §4)
ADGL0212  ProblemScopeInvalid
ADGL0410  CyclicRuleDependency       (05 §2)
ADGL0411  NonCalculableWindow        (05 §3)
ADGL0412  WindowExceedsLookback      (05 §3.1, C4/D3)
ADGL0413  MalformedWindow            (D4)
ADGL0420  UnknownTopology            (C10)
ADGL0421  TopoArity / TopoArgType
ADGL0430  UnknownCapability          (E5)
ADGL0440  OverlappingExclusivity     (C5)
ADGL0441  ExclusivityGroupTooLarge
ADGL0450  BipartiteViolation         (C6)
ADGL0501  EffectInPurePosition       (03 §2)
ADGL0502  RedundantExclusivity       (05 §7)
ADGL0503  DedupWindowTooSmall
```

## 3. Runtime-диагностика (event bus)

```
RuntimeDiag = { id, severity, scope, wm, message, evidence_refs? }

ADGL3001  ActionNoOpInReplay         (G2: request_observation в offline)
ADGL3002  LateEvidence               (08 §4 offline: resolved-absent но пришёл)
ADGL3003  LateEventDropped           (08 §4 live)
ADGL3004  WaitQueueSpill             (D5: pending > max)
ADGL3005  RingBufferSpill            (capacity exceeded)
ADGL3006  TopologyCycleDetected      (upstream_of цикл → False)
ADGL3007  ArithOverflow              (checked-арифметика, теор. недостижимо)
ADGL3101  MaxPendingExceeded         (hard, config)
ADGL3102  MaxCausesPerScopeExceeded
ADGL3103  MaxRuleFiringsPerEventExceeded
ADGL3201  EbpfFilterLoadFailed       (live ActionSink)
ADGL3202  ExternalCheckEnqueueFailed
```

Event bus — sum type + trait sink (`DiagSink`); runtime-диагностика не прерывает
обработку. SARIF-вывод может включать runtime-диагностики как `notifications[]`
(SARIF §3.58).

## 4. NeedMoreData — не ошибка

`Suspended` (WaitQueue, 08 §3) — **не** `CorrelateError`, а нормальное
состояние. UI помечает «Hypothesis Pending Data» (08 §7). Отличие от N-FDL
`NeedMoreBytes`: там — stream-граница, здесь — temporal-window.

## 5. Privacy & telemetry (C9)

- AOT: PII-поля помечены в catalog (`[pii]`, 10 §2); `Intent.pii` mask (06 §2.3).
- Runtime: strict-redaction в evidence JSON (10 §11); telemetry (если включена)
  отправляет только non-PII агрегаты (counts, confidence distributions, scope-types).
- SARIF: `partialFingerprints` содержит `{scope, target_hash, causes}` — target
  hashed (не raw IP) для privacy + stable dedup (C8).

## 6. DoS-mapping (C11)

| DoS-вектор | Diagnostic | Mitigation |
|---|---|---|
| gigant ruleset | `ADGL0102` | lexer 4MiB limit |
| deep nesting | `ADGL0103` | nesting 64 |
| huge correlate window | `ADGL0412` | MAX_LOOKBACK invariant (05 §3.1) |
| many pending | `ADGL3004/3101` | max_pending_per_scope spill |
| event flood | `ADGL3005` | max_ringbuffer capacity |
| topology cycle | `ADGL3006` | visited-set + max_hops |
| rule-firing bomb | `ADGL3103` | max_rule_firings_per_event |

Все — degrade + diagnostic, не паника (07 §9).

## 7. Контракт

1. AOT-ошибки — батч, sorted by span; `ADGL####` stable IDs.
2. Runtime-ошибки — event bus, не прерывают обработку; `ADGL30xx/31xx/32xx`.
3. `Suspended` (WaitQueue) — не ошибка.
4. PII redact в strict; telemetry non-PII only.
5. DoS-векторы → degrade + diagnostic (таблица §6).
6. ariadne rendering для AOT; runtime — structured `RuntimeDiag`.
