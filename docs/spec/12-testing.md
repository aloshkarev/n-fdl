# N-FDL Testing Strategy v1

Полная стратегия верификации: unit, golden, differential, fuzzing, property-based. Реализуется в `nfdl-fuzz` + CI.

## 1. Уровни тестов

### 1.1 Unit tests (per-crate)

- `nfdl-syntax`: lexer токены, парсер-узлы, span, error recovery.
- `nfdl-types`: type-inference кейсы, Option/union/record, qualified access.
- `nfdl-verify`: interval-анализ (граничные underflow/overflow), Tarjan SCC, FSM liveness, plugin-sig.
- `nfdl-bytecode`: кодировка инструкций, jump-table, slot allocation.
- `nfdl-vm`: инструкции READ/EXPR/CONTROL, consumed-check, YIELD.
- `nfdl-stream`: reassembly (OOO, dup, overlap, wrap), resume-equivalence.
- `nfdl-fsm`: key extraction (bidir), transitions, NoMatch, timer-ordering.
- `nfdl-plugin`: FFI-обёртки, manifest validation, free-cb (MIRI + ASAN).

### 1.2 Golden tests (6 протоколов)

Фиксированные input hex + ожидаемый AST/event-вывод (snapshot).

```
tests/golden/
  arp/          input.hex  expected.json
  udp_dns/      ...
  tcp/          ...
  radius/       ...
  diameter/     ...
  gtpu/         ...
```

При каждом изменении IR/bytecode/VM — регрессия ловится. Snapshot-тесты с `insta` или `assert-json-diff`.

### 1.3 Differential tests vs TShark (v1.5)

Корпус PCAP (5 ТБ репрезентативного трафика) прогоняется параллельно:
- N-FDL → canonical JSON AST
- TShark → PDML → canonical JSON

Сравнение деревьев. Расхождения → triage + документирование (интерпретация спеки отличается). Цель M6: ≥95% согласованность на корпусе.

### 1.4 Fuzzing

**DSL parser fuzzing** (`cargo-fuzz`):
- Мутация `.nfdl` → парсер не паникует, только SyntaxError/TypeError/VerificationError.

**Bytecode VM fuzzing**:
- Мутация байткода + пакетов → VM не паникует, не OOB, всегда терминирует (RuntimeSafetyAbort при нарушении).

**Generative packet fuzzing from DSL grammar** (v2):
- Инверсия спеки → структурно-осведомлённый генератор полу-валидных пакетов (AFL++ grammar mutator).

### 1.5 Property-based tests (proptest / quickcheck)

Обязательные свойства (все проверяются на 10k+ случаях):

1. **Conservation of bytes**: `consumed + unconsumed == total_payload` для каждого успешного парса.
2. **Deterministic parse**: одинаковый вход → идентичный выход (битово).
3. **No panics**: ни один вход не вызывает panic (catch_unwind + `forbid(unsafe)`).
4. **No out-of-bounds**: все `Slice` in-bounds (гарантировано safe Rust + fuzzing).
5. **Resume-equivalence (stream)**: разбор потока, разбитого на сегменты произвольно ≡ разбор целиком.
6. **Termination bound**: loop завершается за ≤ `len(slice)` итераций для
   byte-progress loops, либо за ≤ `max_loop_iterations` для доказанных
   zero-byte/carry-progress loops.
7. **Idempotent re-parse**: повторный разбор того же среза даёт тот же результат.
8. **FSM deterministic ordering**: один и тот же вход → идентичная последовательность state-transitions и событий.

### 1.6 MIRI / ASAN / LeakSan

- `nfdl-plugin` FFI-граница (единственный unsafe) — MIRI + ASAN в CI.
- `cargo-leak` / Valgrind на долгоживущих сессиях (Session DB, reassembly).

### 1.7 Conformance harness

`nfdl-cli test --corpus pcap-dir --tshark --golden` — единая точка входа для всех уровней.

## 2. CI gates

- Все unit + property-тесты проходят.
- Golden + differential не регрессируют.
- Fuzzing (1h parser + 1h VM) не находит новых паник/OOB.
- MIRI/ASAN чистые.
- `cargo geiger` показывает unsafe только в `nfdl-plugin`.

## 3. Метрики покрытия

- Line coverage ≥ 85% (tarpaulin).
- Branch coverage на критических путях (bounds, loop progress, resume) — 100%.

## 4. Когда тесты считаются пройденными для milestone

- M0 (ARP): golden ARP + conservation + no-panic + deterministic.
- M4 (TCP stream): resume-equivalence + reassembly properties + differential на TCP-корпусе.
- M6: differential ≥95%, 0 новых паник за 24h fuzzing, все DoS-векторы §12 покрыты property-тестами.