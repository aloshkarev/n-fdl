# ADGL — AirPulse Diagnostic Graph Language

> **Historical / idea archive.** Spec and ADR prose under `docs/idea/` predates the AirPulse production path. **Shipped runtime:** AirPulse loads verified `.adgl` rulesets from `data/diagnostics/` via `airpulse_dsl-*` path deps (`DiagnosisEngine::Adgl`). Do not treat “idea / proposed” wording below as current product status.

**ADGL** (`.adgl`) — декларативный предметно-ориентированный язык для диагностических правил над потоковым направленным ациклическим графом свойств (Core Diagnostic Graph Model, V1). Вторая DSL в репозитории, параллельная N-FDL ([`docs/`](../)): N-FDL описывает бинарные протоколы (разбор → VM байткода → EFSM), ADGL — причинно-следственную диагностику сетевых сбоев (приём событий → разбиение → отметка прогресса → корреляция → вывод/генерация).

> Статус исходного проспекта: **idea archive**. Реализация crates `airpulse_dsl::*` и интеграция в AirPulse — **shipped** (см. баннер выше).

## Назначение

ADGL заменяет плоский TOML-движок правил AirPulse (MCP `user-airpulse`, `airpulse://rules`) потоковым движком на основе DAG с:

- **разбиением по областям агрегации** (Session / Port / ClientMac / Vlan / AccessPoint / Global) — безблокировочный параллелизм и поиск подграфа за O(1);
- **двудольной моделью правил**: `evidence` (Event → Cause) и `decision` (Cause/Problem → Problem/Action);
- **отложенным вычислением по отметке прогресса потока (watermark)** — детерминированное разрешение «парадокса отсутствующих данных» без состояний гонки;
- **коммутативным накоплением степени уверенности (confidence)** с устранением дубликатов по происхождению данных (provenance);
- **автономным синтезом узла неоднозначности (AmbiguityNode)** для конкурирующих гипотез;
- **верификацией на этапе компиляции (AOT)** — пути метрик, временные границы, совместимость областей; диагностика `ADGL####` с привязкой к фрагменту исходного текста (ariadne);
- **выводом SARIF 2.1.0** со стабильными `ruleId` и `partialFingerprints`.

## Отличие от N-FDL

| Аспект | N-FDL | ADGL |
|---|---|---|
| Предметная область | бинарные протоколы | сетевая диагностика |
| Данные | байты потока | события наблюдения |
| Модель выполнения | VM байткода + EFSM | потоковый DAG |
| Состояние | база сессий | разделённое хранилище графа + кольцевой буфер |
| Время | монотонное смещение в буфере | отметка по времени события |
| Результат | AST + события | SARIF/JSON: проблемы и действия |

Соглашения по документации (язык, нумерация разделов спецификации, формат ADR, EBNF, контракты, идентификаторы исправлений `C1..CN`, коды диагностик) согласованы с N-FDL для единообразного чтения обеих DSL.

## Навигация

- **Спецификация**: [spec/01-lexical.md](spec/01-lexical.md) → [spec/13-roadmap.md](spec/13-roadmap.md); грамматика — [spec/02-grammar.ebnf](spec/02-grammar.ebnf).
- **Решения**: [adr/ADR-list-critical-decisions.md](adr/ADR-list-critical-decisions.md) и `ADR-001..ADR-012`.
- **Примеры**: [examples/](examples/) — 9 правил из проспекта V1 (с правками по критическому анализу) и 1 демонстрация AmbiguityNode (10 файлов).
- **Планы**: [plans/](plans/) — миграция, фазы реализации, план тестирования, журнал уточнений корректности.

## Быстрый взгляд

```adgl
ruleset "airpulse.tcp_diagnostics" {
    version = "1.0"
    requires = ["l3-deep", "topology"]

    evidence pmtud_hypothesis {
        scope: Session
        anchor rtx: event(tcp.retransmission_burst) { rtx.segment_size > 1400 }
        correlate ptb: event(icmp.ptb) {
            topo: same_session(rtx.target, ptb.target)
            time: ptb.time in [rtx.time - 500ms, rtx.time + 1s]
        }
        if present(ptb) {
            infer Cause(PmtudBlackhole) { target: rtx.target, weight: +85, evidence: [rtx, ptb] }
        } else {
            infer Cause(PmtudBlackhole) { target: rtx.target, weight: +35, evidence: [rtx] }
            action request_observation(icmp.visibility) { target: rtx.path }
        }
    }

    decision pmtud_verdict {
        scope: Session
        anchor c: Cause(PmtudBlackhole) { c.confidence >= 80 }
        emit Problem(XlIcmpTcpMss) { severity: High, evidence: [c] }
    }
}
```

Полный пример — [examples/01-pmtud-blackhole.adgl](examples/01-pmtud-blackhole.adgl).
