# N-FDL EFSM & Sessions Subsystem v1 / v1.5

Описывает extended finite state machines: извлечение ключа сессии, исполнение
переходов, session DB, таймеры/истечение, эффекты (emit/set), concurrency-модель
и детерминированный порядок. Реализуется в crate `nfdl-fsm`.

Формальная основа: автомат Мили `δ : S × M × C_sess → S' × C_sess' × Effects`
(03 §7). Базовый FSM — v1 (без таймеров: ARP/RADIUS); таймеры/expiration — v1.5.

## 1. Архитектурное место

FSM работает в **control plane**, строго отделённом от data plane (парсера):

- Парсер (data plane) разбирает сообщение `M`, не зная истории сессии (чистота,
  завершаемость — 03 §4.3). `C_sess` парсеру не виден.
- FSM (control plane) получает уже разобранное `M` + доступ к `C_parent` (для
  ключа) и исполняет `δ`. Только FSM читает/пишет `C_sess`.

```
VM.Complete(msg) ──► FSM.feed(msg, ctx) ──► key = extract(msg, C_parent)
                                          ──► session = db.get_or_create(key)
                                          ──► transition(session, msg)
                                          ──► effects: emit / set / timer
```

## 2. Извлечение ключа (key extraction)

### 2.1 Семантика

`key = KeyExpr` вычисляется после успешного разбора `M`, из `C_loc(msg) ∪
C_parent`. Компоненты ключа — pure-выражения (05 §8 проверяет well-formedness).

### 2.2 bidir и bidir_tuple — каноническая нормализация (C4/C10)

`bidir(a, b)` строит направленно-независимый компонент: внутренне сортирует
пару `(a, b)`, так что request (client→server) и response (server→client)
дают идентичный компонент ключа. Без этого корреляция req/resp ломается (была
баг-причина в исходных примерах).

```
bidir(a, b) = if a <= b then (a, b) else (b, a)      // как упорядоченная пара
```

Примеры:

- TCP: `key = bidir_tuple((IPv4.src, src_port), (IPv4.dst, dst_port))`
- RADIUS: `key = (bidir(UDP.src_port, UDP.dst_port), identifier)`

**Критично:** для TCP-like 4/5-tuples пары IP и портов нужно сортировать
согласованно. Независимая нормализация компонентов запрещена, потому что потоки
`10.0.0.1:8000 -> 10.0.0.2:80` и
`10.0.0.1:80 -> 10.0.0.2:8000` схлопываются в один ключ. Для таких протоколов
используется `bidir_tuple((ip_a,port_a),(ip_b,port_b))`, который сортирует
endpoint tuple как единицу. Для RADIUS (ключ + identifier) независимая
нормализация портов достаточна.

### 2.3 Хеширование и FlowKey

Composite key сериализуется в стабильный `FlowKey` (фиксированный byte-layout) →
хешируется (ahash/SipHash для DoS-стойкости при недоверенном вводе). Скалярные
компоненты копируются по значению (не zero-copy — ключ переживает пакет).

## 3. Session DB

```
SessionDb {
    map: HashMap<FlowKey, SessionContext>,   // SipHash против hash-flooding
    lru: LruIndex,                            // для эвикции (§6)
    count: usize,
}
SessionContext {                              // 07 §5.4 — owned, без 'pkt
    machine: FsmId,
    state: StateId,
    vars: HashMap<Sym, OwnedValue>,           // set-переменные (req_auth и т.п.)
    timers: TimerSet,                          // v1.5
    created: Instant, last_seen: Instant,
}
```

`get_or_create(key)`: существующая сессия → вернуть; иначе создать в стартовом
состоянии FSM. Стартовое состояние — первое объявленное `state` (или явно
помеченное; ADR-кандидат на явный `initial`).

## 4. Исполнение перехода (transition)

```
fn transition(session, msg):
    let st = fsm.states[session.state]
    for tr in st.transitions:                 # порядок объявления = приоритет
        if tr.on == msg.type
           && eval(tr.guard, C_loc(msg) ∪ session.vars) == true:   # guard: pure
            execute_actions(tr.actions, session, msg)    # по порядку
            session.state = tr.target
            session.last_seen = now
            return Matched
    return NoMatch    # сессия не меняется (опц. diagnostic 'unexpected message in state')
```

- **Ordered choice**: первый matching transition выигрывает (детерминизм).
- **Guard** — pure-выражение над полями сообщения и session-переменными
  (`is_syn == 1`, `code == 2`). Не двигает offset, без эффектов.
- **NoMatch** не ошибка — легитимно (например, дублирующий ACK в ESTABLISHED).

## 5. Эффекты (actions)

Исполняются по порядку в теле transition:

```
emit E;                  -> событие E в event bus (тип, ключ-hash, timestamp; без PII)
set v = expr;            -> session.vars[v] = owned_copy(eval(expr))   # КОПИЯ (07 §5.4)
start_timer(name, dur);  -> session.timers.arm(name, now+dur)          # v1.5
cancel_timer(name);      -> session.timers.disarm(name)                # v1.5
```

`set` — единственная мутация `C_sess`; значение копируется в `OwnedValue`
(bytes → `Box<[u8]>`), т.к. переживает пакет. `emit` порядок сохраняется →
детерминированная последовательность событий.

## 6. Таймеры, истечение, эвикция (v1.5)

### 6.1 Timer model

Hierarchical timing wheel (или min-heap по deadline для малого числа таймеров).
Тикает в том же event loop (07 §9). При срабатывании таймера — **timer-transition**:

```
on timer(name) [guard g] -> S' { actions };     # синтаксис v1.5, расширение FSM
```

Срабатывание = событие, обрабатываемое как `feed` с псевдо-сообщением timeout →
может сменить состояние, эмитить событие, удалить сессию.

### 6.2 Expiration

- **Idle timeout**: `now - last_seen > idle_timeout` → emit `SESSION_EXPIRED`,
  удалить сессию. Проверяется лениво (при доступе) + периодическим sweep.
- **Hard cap**: `max_sessions` достигнут → LRU-эвикция (drop наименее активной)
  + diagnostic. Защита от session-explosion DoS (12 §плана).

### 6.3 Deterministic timer ordering

При совпадении deadline нескольких таймеров — порядок по (deadline, insertion_id)
тотальный. Пакет vs таймер при равном timestamp: **пакет обрабатывается раньше**
таймера (зафиксировано; ADR-кандидат). Это нужно для воспроизводимости
differential-тестов.

## 7. Concurrency model

### 7.1 v1: single-threaded event loop

Все пакеты/сегменты/таймеры — последовательно в одном loop. Session DB без
блокировок, без гонок. Тотальный детерминированный порядок обработки и эмиссии.
Простота > throughput для v1.

### 7.2 v2: sharded runtime

Партиционирование по `FlowKey`-hash на N shard-потоков; каждый shard владеет
своей партицией session DB (share-nothing, без блокировок внутри shard). Внутри
shard сохраняется детерминизм. Кросс-flow корреляция (редкая) — через
message-passing между shard. `Arc<ProgramImage>` разделяется read-only.

## 8. Детерминированный порядок (контракт)

1. Внутри пакета: ordered choice в guard'ах transitions.
2. Между пакетами: строгий input-order (single thread v1 / per-shard v2).
3. Таймеры: тотальный порядок (deadline, insertion_id); пакет > таймер при равенстве.
4. Эмиссия событий: в порядке возникновения эффектов.
5. Один и тот же вход (поток пакетов + тайминги) → идентичная
   последовательность state-transitions и событий (property: deterministic FSM).

## 9. Взаимодействие с другими подсистемами

- **Stream (08)**: для TCP FSM `Connection` получает `Segment` от L4-разбора
  (не от L7-stream). FSM управляет жизненным циклом flow косвенно (FIN →
  FIN_WAIT → close → flow можно эвиктить).
- **Dispatcher (07)**: предоставляет `C_parent` для key extraction (`IPv4.src`).
- **Event bus (11)**: приёмник `emit`; события структурированы, без сырого payload.

## 10. Контракт подсистемы

1. Парсер не зависит от `C_sess` (data/control plane разделены).
2. Key extraction детерминирован; `bidir_tuple` корректно связывает endpoint
   req/resp без коллизии независимых IP/port components.
3. `set` всегда копирует (нет `'pkt` в `C_sess`) → нет висячих ссылок.
4. Transitions — ordered choice; guard pure; NoMatch не ошибка.
5. Session DB bounded (`max_sessions` + idle-timeout + LRU); hash-flood стойкость.
6. Детерминированный тотальный порядок событий (для differential-тестов).
